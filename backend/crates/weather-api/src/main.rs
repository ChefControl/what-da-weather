mod notifier;
mod status;

use std::convert::Infallible;
use std::sync::Arc;
use std::time::Duration;

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use futures::stream::{Stream, StreamExt};
use serde::Deserialize;
use tokio_stream::wrappers::BroadcastStream;
use tower_http::services::{ServeDir, ServeFile};
use tower_http::trace::TraceLayer;

use wdw_core::config::AppConfig;
use wdw_core::event::{EvaluationEvent, VerdictSource};
use wdw_core::llm::LlmClient;
use wdw_core::metrics;
use wdw_core::publish::Publisher;
use wdw_core::rules;
use wdw_core::weather::{OpenMeteo, WeatherError, WeatherProvider};

use notifier::Notifier;

pub struct AppState {
    config: AppConfig,
    provider: Box<dyn WeatherProvider>,
    llm: LlmClient,
    publisher: Publisher,
    notifier: Notifier,
    es_url: String,
    http: reqwest::Client,
}

type SharedState = Arc<AppState>;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,tower_http=warn".into()),
        )
        .init();
    metrics::touch();

    let config_path =
        std::env::var("ACTIVITIES_FILE").unwrap_or_else(|_| "config/activities.yaml".to_string());
    let config = AppConfig::load(&config_path)?;
    tracing::info!(
        activities = config.activities.len(),
        cities = config.scheduler.cities.len(),
        "loaded {config_path}"
    );

    let state: SharedState = Arc::new(AppState {
        config,
        provider: Box::new(OpenMeteo::from_env()),
        llm: LlmClient::from_env(),
        publisher: Publisher::from_env(),
        notifier: Notifier::new(),
        es_url: std::env::var("ES_URL").unwrap_or_else(|_| "http://elasticsearch:9200".to_string()),
        http: reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .build()?,
    });

    let static_dir = std::env::var("STATIC_DIR").unwrap_or_else(|_| "static".to_string());
    let spa =
        ServeDir::new(&static_dir).fallback(ServeFile::new(format!("{static_dir}/index.html")));

    let app = Router::new()
        .route("/api/evaluate", post(evaluate))
        .route("/api/status", get(status_handler))
        .route("/api/activities", get(activities))
        .route("/api/events", get(sse_events))
        .route("/healthz", get(|| async { "ok" }))
        .route("/metrics", get(|| async { metrics::render() }))
        .fallback_service(spa)
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let bind = std::env::var("BIND_ADDR").unwrap_or_else(|_| "0.0.0.0:8080".to_string());
    let listener = tokio::net::TcpListener::bind(&bind).await?;
    tracing::info!("weather-api listening on {bind}");
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("install ctrl-c handler");
    };
    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("install sigterm handler")
            .recv()
            .await;
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
    tracing::info!("shutdown signal received");
}

// ---------- error type ----------

enum ApiError {
    UnknownActivity(String),
    CityNotFound(String),
    Provider(String),
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (code, msg) = match self {
            ApiError::UnknownActivity(a) => {
                (StatusCode::NOT_FOUND, format!("unknown activity: {a}"))
            }
            ApiError::CityNotFound(c) => (StatusCode::NOT_FOUND, format!("city not found: {c}")),
            ApiError::Provider(e) => (
                StatusCode::BAD_GATEWAY,
                format!("weather provider unavailable: {e}"),
            ),
        };
        (code, Json(serde_json::json!({"error": msg}))).into_response()
    }
}

// ---------- handlers ----------

#[derive(Deserialize)]
struct EvaluateRequest {
    city: String,
    activity: String,
    #[serde(default)]
    trigger: Option<String>,
}

async fn evaluate(
    State(state): State<SharedState>,
    Json(req): Json<EvaluateRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let trigger = match req.trigger.as_deref() {
        Some("scheduler") => "scheduler",
        _ => "user",
    };
    let activity = state
        .config
        .activities
        .get(&req.activity)
        .ok_or_else(|| ApiError::UnknownActivity(req.activity.clone()))?;

    let (location, weather) = state
        .provider
        .fetch(req.city.trim())
        .await
        .map_err(|e| match e {
            WeatherError::CityNotFound(c) => ApiError::CityNotFound(c),
            other => ApiError::Provider(other.to_string()),
        })?;

    let gate = rules::evaluate_gate(activity, &weather);
    let (recommended, source, reasoning, llm_latency_ms) = if !gate.passed {
        (
            false,
            VerdictSource::RulesGate,
            format!("Blocked by hard constraints: {}", gate.failures.join("; ")),
            None,
        )
    } else {
        match state
            .llm
            .verdict(&activity.name, &activity.prompt, &weather)
            .await
        {
            Ok((verdict, latency_ms)) => (
                verdict.recommended,
                VerdictSource::Llm,
                verdict.reasoning,
                Some(latency_ms),
            ),
            Err(e) => {
                // Preference nuance lives in the LLM now; without it the honest
                // degraded answer is "possible": every hard constraint passed.
                tracing::warn!(error = %e, "llm unavailable, degrading to gate-only verdict");
                metrics::LLM_FALLBACKS.inc();
                (
                    true,
                    VerdictSource::Fallback,
                    "All hard constraints pass; the LLM advisor is unavailable, so no \
                     preference ranking was applied."
                        .to_string(),
                    None,
                )
            }
        }
    };

    let event = EvaluationEvent {
        event_id: uuid::Uuid::new_v4().to_string(),
        timestamp: chrono::Utc::now(),
        trigger: trigger.to_string(),
        city: location.name,
        country: location.country,
        latitude: location.latitude,
        longitude: location.longitude,
        activity: req.activity.clone(),
        activity_name: activity.name.clone(),
        weather,
        gate_passed: gate.passed,
        gate_failures: gate.failures,
        recommended,
        source,
        reasoning,
        llm_latency_ms,
    };

    metrics::EVALUATIONS
        .with_label_values(&[
            &event.activity,
            if recommended {
                "recommended"
            } else {
                "not_recommended"
            },
            source.as_str(),
            trigger,
        ])
        .inc();

    // Publish before responding: the caller learns whether the event reached
    // the durable pipeline. Bounded overall so a broker outage cannot hang the API.
    let published = match tokio::time::timeout(
        Duration::from_secs(10),
        state.publisher.publish(&event),
    )
    .await
    {
        Ok(Ok(())) => true,
        Ok(Err(e)) => {
            tracing::error!(error = %e, "event publish failed after retries");
            false
        }
        Err(_) => {
            tracing::error!("event publish timed out");
            false
        }
    };
    if !published {
        metrics::PUBLISH_DROPPED.inc();
    }

    state.notifier.observe(&event);

    Ok(Json(
        serde_json::json!({ "event": event, "published": published }),
    ))
}

async fn status_handler(State(state): State<SharedState>) -> Json<serde_json::Value> {
    let memory: Vec<serde_json::Value> = state
        .notifier
        .snapshot()
        .into_iter()
        .filter_map(|e| serde_json::to_value(e).ok())
        .collect();
    let (es_items, es_ok) = match status::latest_from_es(&state.http, &state.es_url).await {
        Ok(items) => (items, true),
        Err(e) => {
            tracing::warn!(error = %e, "elasticsearch status query failed; serving memory only");
            (Vec::new(), false)
        }
    };
    let items = status::merge_latest(es_items, memory);
    Json(serde_json::json!({ "items": items, "elasticsearch": es_ok }))
}

async fn activities(State(state): State<SharedState>) -> Json<serde_json::Value> {
    let list: Vec<serde_json::Value> = state
        .config
        .activities
        .iter()
        .map(|(key, a)| {
            let mut required: Vec<String> =
                a.required.iter().map(|c| c.description.clone()).collect();
            if a.require_daylight {
                required.insert(0, "Only while the sun is up at the location".to_string());
            }
            serde_json::json!({
                "key": key,
                "name": a.name,
                "required": required,
                "prompt": a.prompt,
            })
        })
        .collect();
    Json(serde_json::json!({
        "activities": list,
        "cities": state.config.scheduler.cities,
    }))
}

struct SseGuard;

impl SseGuard {
    fn new() -> Self {
        metrics::SSE_CLIENTS.inc();
        SseGuard
    }
}

impl Drop for SseGuard {
    fn drop(&mut self) {
        metrics::SSE_CLIENTS.dec();
    }
}

async fn sse_events(
    State(state): State<SharedState>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let guard = SseGuard::new();
    let stream = BroadcastStream::new(state.notifier.subscribe())
        .filter_map(|msg| futures::future::ready(msg.ok()))
        .map(move |msg| {
            let _connected = &guard; // gauge decrements when the stream drops
            Ok(Event::default().event("recommendation").data(msg))
        });
    Sse::new(stream).keep_alive(KeepAlive::default())
}
