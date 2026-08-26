use once_cell::sync::Lazy;
use prometheus::{
    register_histogram, register_int_counter, register_int_counter_vec, register_int_gauge,
    Encoder, Histogram, IntCounter, IntCounterVec, IntGauge, TextEncoder,
};

pub static EVALUATIONS: Lazy<IntCounterVec> = Lazy::new(|| {
    register_int_counter_vec!(
        "evaluations_total",
        "Activity evaluations by outcome",
        &["activity", "verdict", "source", "trigger"]
    )
    .unwrap()
});

// Dedicated LLM observability metrics (DESIGN.md §7 bonus).
pub static LLM_REQUESTS: Lazy<IntCounterVec> = Lazy::new(|| {
    register_int_counter_vec!("llm_requests_total", "LLM verdict requests", &["status"]).unwrap()
});
pub static LLM_ERRORS: Lazy<IntCounterVec> = Lazy::new(|| {
    register_int_counter_vec!("llm_errors_total", "LLM failures by kind", &["kind"]).unwrap()
});
pub static LLM_LATENCY: Lazy<Histogram> = Lazy::new(|| {
    register_histogram!(
        "llm_latency_seconds",
        "End-to-end LLM verdict latency",
        vec![0.1, 0.25, 0.5, 1.0, 2.0, 4.0, 8.0, 16.0, 30.0, 60.0]
    )
    .unwrap()
});
pub static LLM_FALLBACKS: Lazy<IntCounter> = Lazy::new(|| {
    register_int_counter!(
        "llm_fallbacks_total",
        "Verdicts served by rule-based fallback"
    )
    .unwrap()
});

pub static PUBLISH_FAILURES: Lazy<IntCounter> = Lazy::new(|| {
    register_int_counter!(
        "publish_failures_total",
        "Failed RabbitMQ publish attempts (retried)"
    )
    .unwrap()
});
pub static PUBLISH_DROPPED: Lazy<IntCounter> = Lazy::new(|| {
    register_int_counter!(
        "publish_dropped_total",
        "Events dropped after exhausting publish retries"
    )
    .unwrap()
});

pub static NOTIFICATIONS_SENT: Lazy<IntCounter> = Lazy::new(|| {
    register_int_counter!(
        "notifications_sent_total",
        "SSE notifications broadcast on became-recommended transitions"
    )
    .unwrap()
});
pub static SSE_CLIENTS: Lazy<IntGauge> =
    Lazy::new(|| register_int_gauge!("sse_clients", "Currently connected SSE clients").unwrap());

/// Force registration of every metric so /metrics shows zeroed series from startup.
pub fn touch() {
    Lazy::force(&EVALUATIONS);
    Lazy::force(&LLM_REQUESTS);
    Lazy::force(&LLM_ERRORS);
    Lazy::force(&LLM_LATENCY);
    Lazy::force(&LLM_FALLBACKS);
    Lazy::force(&PUBLISH_FAILURES);
    Lazy::force(&PUBLISH_DROPPED);
    Lazy::force(&NOTIFICATIONS_SENT);
    Lazy::force(&SSE_CLIENTS);
}

/// Render the default registry in Prometheus text exposition format.
pub fn render() -> String {
    let metric_families = prometheus::gather();
    let mut buf = Vec::new();
    TextEncoder::new()
        .encode(&metric_families, &mut buf)
        .expect("encode metrics");
    String::from_utf8(buf).expect("metrics are utf-8")
}
