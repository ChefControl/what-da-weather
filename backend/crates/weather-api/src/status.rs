use std::collections::HashMap;

use serde_json::{json, Value};

/// Fetch the latest indexed evaluation per (city, activity) from Elasticsearch.
///
/// The index pattern `weather-recs-2*` matches the daily data indices
/// (weather-recs-YYYY.MM.dd) but not weather-recs-dlq.
pub async fn latest_from_es(http: &reqwest::Client, es_url: &str) -> anyhow::Result<Vec<Value>> {
    let body = json!({
        "size": 0,
        "aggs": {
            "pairs": {
                "composite": {
                    "size": 500,
                    "sources": [
                        {"city": {"terms": {"field": "city.keyword"}}},
                        {"activity": {"terms": {"field": "activity.keyword"}}}
                    ]
                },
                "aggs": {
                    "latest": {
                        "top_hits": {
                            "size": 1,
                            "sort": [{"@timestamp": {"order": "desc"}}]
                        }
                    }
                }
            }
        }
    });

    let resp: Value = http
        .post(format!(
            "{es_url}/weather-recs-2*/_search?ignore_unavailable=true&allow_no_indices=true"
        ))
        .json(&body)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    let buckets = resp
        .pointer("/aggregations/pairs/buckets")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    Ok(buckets
        .iter()
        .filter_map(|b| b.pointer("/latest/hits/hits/0/_source").cloned())
        .collect())
}

/// Merge ES results with the in-memory snapshot, keeping the newest event per
/// (city, activity). Memory covers events Logstash hasn't indexed yet and full
/// ES outages; ES covers state from before the last API restart.
pub fn merge_latest(es_items: Vec<Value>, memory_items: Vec<Value>) -> Vec<Value> {
    let mut best: HashMap<(String, String), Value> = HashMap::new();
    for item in es_items.into_iter().chain(memory_items.into_iter()) {
        let key = (
            item.get("city")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            item.get("activity")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
        );
        let ts = |v: &Value| {
            v.get("timestamp")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string()
        };
        match best.get(&key) {
            // RFC3339 timestamps in UTC compare correctly as strings.
            Some(existing) if ts(existing) >= ts(&item) => {}
            _ => {
                best.insert(key, item);
            }
        }
    }
    let mut items: Vec<Value> = best.into_values().collect();
    items.sort_by(|a, b| {
        let k = |v: &Value| {
            (
                v.get("city")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                v.get("activity")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
            )
        };
        k(a).cmp(&k(b))
    });
    items
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(city: &str, activity: &str, ts: &str, recommended: bool) -> Value {
        json!({"city": city, "activity": activity, "timestamp": ts, "recommended": recommended})
    }

    #[test]
    fn merge_prefers_newer_timestamp() {
        let es = vec![item("Tel Aviv", "matkot", "2026-08-26T10:00:00Z", false)];
        let mem = vec![item("Tel Aviv", "matkot", "2026-08-26T10:10:00Z", true)];
        let merged = merge_latest(es, mem);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0]["recommended"], json!(true));
    }

    #[test]
    fn merge_keeps_disjoint_pairs() {
        let es = vec![item("Tel Aviv", "matkot", "2026-08-26T10:00:00Z", false)];
        let mem = vec![item("Haifa", "nature", "2026-08-26T10:05:00Z", true)];
        let merged = merge_latest(es, mem);
        assert_eq!(merged.len(), 2);
    }

    #[test]
    fn merge_is_sorted_and_stable_for_older_memory() {
        let es = vec![item("B City", "x", "2026-08-26T12:00:00Z", true)];
        let mem = vec![
            item("B City", "x", "2026-08-26T11:00:00Z", false),
            item("A City", "x", "2026-08-26T11:00:00Z", false),
        ];
        let merged = merge_latest(es, mem);
        assert_eq!(merged.len(), 2);
        assert_eq!(merged[0]["city"], json!("A City"));
        assert_eq!(merged[1]["recommended"], json!(true)); // newer ES doc won
    }
}
