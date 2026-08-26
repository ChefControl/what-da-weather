use std::collections::HashMap;
use std::sync::Mutex;

use tokio::sync::broadcast;
use wdw_core::event::EvaluationEvent;
use wdw_core::metrics;

/// In-process notifier (DESIGN.md D6): keeps the last verdict per
/// (city, activity) and broadcasts an SSE payload only on the
/// not-recommended -> recommended transition. A restart silently re-baselines:
/// the first observation of a pair never notifies.
pub struct Notifier {
    latest: Mutex<HashMap<(String, String), EvaluationEvent>>,
    tx: broadcast::Sender<String>,
}

impl Default for Notifier {
    fn default() -> Self {
        Self::new()
    }
}

impl Notifier {
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(64);
        Self {
            latest: Mutex::new(HashMap::new()),
            tx,
        }
    }

    /// Record an evaluation; returns true when a notification was broadcast.
    pub fn observe(&self, event: &EvaluationEvent) -> bool {
        let key = (event.city.clone(), event.activity.clone());
        let mut map = self.latest.lock().expect("notifier lock");
        let became_recommended =
            event.recommended && matches!(map.get(&key), Some(prev) if !prev.recommended);
        map.insert(key, event.clone());
        drop(map);

        if became_recommended {
            let payload = serde_json::json!({
                "type": "became_recommended",
                "city": event.city,
                "activity": event.activity,
                "activity_name": event.activity_name,
                "reasoning": event.reasoning,
                "timestamp": event.timestamp,
            })
            .to_string();
            metrics::NOTIFICATIONS_SENT.inc();
            // Send only fails when no SSE client is connected — fine by design.
            let _ = self.tx.send(payload);
        }
        became_recommended
    }

    pub fn subscribe(&self) -> broadcast::Receiver<String> {
        self.tx.subscribe()
    }

    /// Current in-memory last-verdicts, used as the /api/status fallback and
    /// to overlay events Logstash hasn't indexed yet.
    pub fn snapshot(&self) -> Vec<EvaluationEvent> {
        self.latest
            .lock()
            .expect("notifier lock")
            .values()
            .cloned()
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wdw_core::event::VerdictSource;
    use wdw_core::weather::WeatherSnapshot;

    fn event(city: &str, activity: &str, recommended: bool) -> EvaluationEvent {
        EvaluationEvent {
            event_id: "test".into(),
            timestamp: chrono::Utc::now(),
            trigger: "user".into(),
            city: city.into(),
            country: None,
            latitude: 0.0,
            longitude: 0.0,
            activity: activity.into(),
            activity_name: activity.into(),
            weather: WeatherSnapshot {
                temperature_c: 20.0,
                wind_kmh: 5.0,
                precipitation_mm: 0.0,
                visibility_km: 20.0,
                weather_code: 0,
                is_day: true,
            },
            gate_passed: true,
            gate_failures: vec![],
            recommended,
            source: VerdictSource::Llm,
            reasoning: "test".into(),
            llm_latency_ms: None,
        }
    }

    #[test]
    fn first_observation_never_notifies() {
        let n = Notifier::new();
        assert!(!n.observe(&event("Tel Aviv", "matkot", true)));
    }

    #[test]
    fn notifies_only_on_became_recommended_edge() {
        let n = Notifier::new();
        let _rx = n.subscribe();
        assert!(!n.observe(&event("Tel Aviv", "matkot", false)));
        assert!(n.observe(&event("Tel Aviv", "matkot", true))); // false -> true: notify
        assert!(!n.observe(&event("Tel Aviv", "matkot", true))); // true -> true: silent
        assert!(!n.observe(&event("Tel Aviv", "matkot", false))); // true -> false: silent
        assert!(n.observe(&event("Tel Aviv", "matkot", true))); // false -> true again
    }

    #[test]
    fn pairs_are_tracked_independently() {
        let n = Notifier::new();
        n.observe(&event("Tel Aviv", "matkot", false));
        assert!(!n.observe(&event("Haifa", "matkot", true))); // different city: first observation
        assert!(!n.observe(&event("Tel Aviv", "nature", true))); // different activity: first observation
        assert!(n.observe(&event("Tel Aviv", "matkot", true)));
        assert_eq!(n.snapshot().len(), 3);
    }
}
