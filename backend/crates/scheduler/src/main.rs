//! Scheduler (DESIGN.md §3 flow 2): every `interval_minutes` it walks the
//! configured (city x activity) pairs and calls the same /api/evaluate
//! endpoint the UI uses, so both paths exercise identical code.

use std::time::Duration;

use axum::routing::get;
use axum::Router;
use once_cell::sync::Lazy;
use prometheus::{register_int_counter, register_int_counter_vec, IntCounter, IntCounterVec};
use wdw_core::config::AppConfig;
use wdw_core::metrics;

static TICKS: Lazy<IntCounter> = Lazy::new(|| {
    register_int_counter!("scheduler_ticks_total", "Completed scheduler ticks").unwrap()
});
static EVAL_REQUESTS: Lazy<IntCounterVec> = Lazy::new(|| {
    register_int_counter_vec!(
        "scheduler_evaluations_total",
        "Evaluation requests issued by the scheduler",
        &["status"]
    )
    .unwrap()
});

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();
    Lazy::force(&TICKS);
    Lazy::force(&EVAL_REQUESTS);

    let config_path =
        std::env::var("ACTIVITIES_FILE").unwrap_or_else(|_| "config/activities.yaml".to_string());
    let config = AppConfig::load(&config_path)?;
    let api_url =
        std::env::var("API_URL").unwrap_or_else(|_| "http://weather-api:8080".to_string());

    // /metrics + /healthz for Prometheus and the compose healthcheck.
    let bind = std::env::var("BIND_ADDR").unwrap_or_else(|_| "0.0.0.0:8080".to_string());
    let listener = tokio::net::TcpListener::bind(&bind).await?;
    tracing::info!("scheduler metrics listening on {bind}");
    tokio::spawn(async move {
        let app = Router::new()
            .route("/healthz", get(|| async { "ok" }))
            .route("/metrics", get(|| async { metrics::render() }));
        if let Err(e) = axum::serve(listener, app).await {
            tracing::error!(error = %e, "metrics server exited");
        }
    });

    // Must exceed the API's additive worst case (~193s: weather retries ~62s
    // + LLM 2x60s + 0.5s sleep + 10s publish), or a slow-but-alive LLM makes
    // every scheduled evaluation report a client-side error the API never had.
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(210))
        .build()?;
    // The config file carries the default; SCHEDULER_INTERVAL_MINUTES
    // overrides it per deployment without editing the mounted YAML.
    let interval_minutes = std::env::var("SCHEDULER_INTERVAL_MINUTES")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .filter(|m| *m >= 1)
        .unwrap_or(config.scheduler.interval_minutes);
    let tick_window = Duration::from_secs(interval_minutes * 60);
    let mut interval = tokio::time::interval(tick_window);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    // Spread the pairs across the tick window (one slot each) instead of
    // firing them back-to-back: a user evaluation arriving mid-tick queues
    // behind at most ~one in-flight verdict on the single-slot LLM, and the
    // per-tick latency spike flattens out in Prometheus.
    let pair_count = (config.scheduler.cities.len() * config.activities.len()).max(1);
    let slot = tick_window / pair_count as u32;

    tracing::info!(
        interval_minutes,
        slot_seconds = slot.as_secs_f64(),
        cities = ?config.scheduler.cities,
        activities = config.activities.len(),
        "scheduler started"
    );
    loop {
        interval.tick().await; // first tick fires immediately
        run_tick(&client, &api_url, &config, slot).await;
        TICKS.inc();
    }
}

async fn run_tick(client: &reqwest::Client, api_url: &str, config: &AppConfig, slot: Duration) {
    let mut slots = tokio::time::interval(slot);
    // An evaluation running past its slot just delays the next one; skipped
    // slots must not burst afterwards or the spreading is lost.
    slots.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    for city in &config.scheduler.cities {
        for (key, activity) in &config.activities {
            slots.tick().await; // first slot fires immediately
            let body = serde_json::json!({
                "city": city,
                "activity": key,
                "trigger": "scheduler",
            });
            let result = client
                .post(format!("{api_url}/api/evaluate"))
                .json(&body)
                .send()
                .await
                .and_then(|r| r.error_for_status());
            match result {
                Ok(resp) => {
                    EVAL_REQUESTS.with_label_values(&["ok"]).inc();
                    let verdict =
                        resp.json::<serde_json::Value>().await.ok().and_then(|v| {
                            v.pointer("/event/recommended").and_then(|r| r.as_bool())
                        });
                    tracing::info!(city, activity = %activity.name, ?verdict, "evaluated");
                }
                Err(e) => {
                    // The next tick retries the pair; the API's own durability
                    // layers cover data loss, so a failed request is only logged.
                    EVAL_REQUESTS.with_label_values(&["error"]).inc();
                    tracing::warn!(city, activity = %activity.name, error = %e, "evaluation failed");
                }
            }
        }
    }
}
