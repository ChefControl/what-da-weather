use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::weather::WeatherSnapshot;

/// Where the final verdict came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum VerdictSource {
    /// A required constraint failed; the LLM was never consulted.
    RulesGate,
    /// The local LLM produced the verdict.
    Llm,
    /// The LLM was unavailable/unparseable; rule-based fallback produced it.
    Fallback,
    /// The LLM's verdict contradicted the deterministic condition policy and
    /// was overridden by code (the consistency guard).
    Corrected,
}

impl VerdictSource {
    pub fn as_str(&self) -> &'static str {
        match self {
            VerdictSource::RulesGate => "rules-gate",
            VerdictSource::Llm => "llm",
            VerdictSource::Fallback => "fallback",
            VerdictSource::Corrected => "corrected",
        }
    }
}

/// The full evaluation record: returned to the caller and published verbatim
/// to RabbitMQ, from where Logstash indexes it into Elasticsearch (R3).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvaluationEvent {
    /// Also used as the Elasticsearch document id, making pipeline redelivery idempotent.
    pub event_id: String,
    pub timestamp: DateTime<Utc>,
    /// "user" (on-demand from the UI) or "scheduler".
    pub trigger: String,
    pub city: String,
    pub country: Option<String>,
    pub latitude: f64,
    pub longitude: f64,
    pub activity: String,
    pub activity_name: String,
    pub weather: WeatherSnapshot,
    pub gate_passed: bool,
    pub gate_failures: Vec<String>,
    pub recommended: bool,
    pub source: VerdictSource,
    pub reasoning: String,
    pub llm_latency_ms: Option<u64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_serializes_kebab_case() {
        assert_eq!(
            serde_json::to_string(&VerdictSource::RulesGate).unwrap(),
            "\"rules-gate\""
        );
        assert_eq!(
            serde_json::to_string(&VerdictSource::Llm).unwrap(),
            "\"llm\""
        );
        assert_eq!(
            serde_json::to_string(&VerdictSource::Fallback).unwrap(),
            "\"fallback\""
        );
    }
}
