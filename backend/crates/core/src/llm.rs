use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::config::Constraint;
use crate::metrics;
use crate::rules::constraint_satisfied;
use crate::weather::WeatherSnapshot;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LlmVerdict {
    pub recommended: bool,
    pub reasoning: String,
}

#[derive(Debug, thiserror::Error)]
pub enum LlmError {
    #[error("llm request failed: {0}")]
    Http(#[from] reqwest::Error),
    #[error("llm returned unparseable output: {0}")]
    Unparseable(String),
}

impl LlmError {
    fn kind(&self) -> &'static str {
        match self {
            LlmError::Http(e) if e.is_timeout() => "timeout",
            LlmError::Http(e) if e.is_connect() => "connect",
            LlmError::Http(_) => "http",
            LlmError::Unparseable(_) => "unparseable",
        }
    }
}

/// Client for the local llama.cpp server's OpenAI-compatible chat endpoint.
pub struct LlmClient {
    base_url: String,
    attempts: u32,
    client: reqwest::Client,
}

impl LlmClient {
    pub fn new(base_url: String, timeout: Duration, attempts: u32) -> Self {
        let client = reqwest::Client::builder()
            .timeout(timeout)
            .build()
            .expect("reqwest client");
        Self {
            base_url,
            attempts,
            client,
        }
    }

    pub fn from_env() -> Self {
        let base_url = std::env::var("LLM_URL").unwrap_or_else(|_| "http://llm:8080".to_string());
        let timeout_secs: u64 = std::env::var("LLM_TIMEOUT_SECS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(60);
        let attempts: u32 = std::env::var("LLM_ATTEMPTS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(2);
        Self::new(base_url, Duration::from_secs(timeout_secs), attempts)
    }

    /// Ask the LLM for a verdict. Returns the verdict and its latency in ms.
    /// Records the dedicated LLM metrics on both success and failure.
    pub async fn verdict(
        &self,
        activity_name: &str,
        guidance: &str,
        conditions: &[Constraint],
        weather: &WeatherSnapshot,
    ) -> Result<(LlmVerdict, u64), LlmError> {
        let started = Instant::now();
        let mut last_err: Option<LlmError> = None;
        for attempt in 0..self.attempts {
            if attempt > 0 {
                tokio::time::sleep(Duration::from_millis(500)).await;
            }
            match self
                .request_once(activity_name, guidance, conditions, weather)
                .await
            {
                Ok(verdict) => {
                    let elapsed = started.elapsed();
                    metrics::LLM_REQUESTS.with_label_values(&["ok"]).inc();
                    metrics::LLM_LATENCY.observe(elapsed.as_secs_f64());
                    return Ok((verdict, elapsed.as_millis() as u64));
                }
                Err(e) => {
                    tracing::warn!(error = %e, attempt = attempt + 1, "llm attempt failed");
                    metrics::LLM_ERRORS.with_label_values(&[e.kind()]).inc();
                    last_err = Some(e);
                }
            }
        }
        metrics::LLM_REQUESTS.with_label_values(&["error"]).inc();
        Err(last_err.expect("at least one attempt"))
    }

    async fn request_once(
        &self,
        activity_name: &str,
        guidance: &str,
        conditions: &[Constraint],
        weather: &WeatherSnapshot,
    ) -> Result<LlmVerdict, LlmError> {
        let body = serde_json::json!({
            "model": "local",
            "temperature": 0.0,
            "max_tokens": 250,
            "response_format": {"type": "json_object"},
            "messages": [
                {
                    "role": "system",
                    "content": "You are a concise weather-activity advisor. The user names an activity, guidance on how to decide, the current weather, and condition checks ALREADY COMPUTED by the system. Trust the computed results exactly - never recompute or second-guess them. Apply the guidance to those results and decide. Respond with a single JSON object of the form {\"reasoning\": \"one or two sentences citing the deciding conditions, ending with exactly 'Conclusion: recommended.' or 'Conclusion: not recommended.'\", \"recommended\": true|false} and nothing else - reasoning FIRST, then recommended. recommended is true when the reasoning ends 'Conclusion: recommended.' and false when it ends 'Conclusion: not recommended.'"
                },
                {"role": "user", "content": build_prompt(activity_name, guidance, conditions, weather)}
            ]
        });

        #[derive(Deserialize)]
        struct ChatResponse {
            choices: Vec<Choice>,
        }
        #[derive(Deserialize)]
        struct Choice {
            message: Message,
        }
        #[derive(Deserialize)]
        struct Message {
            content: String,
        }

        let resp: ChatResponse = self
            .client
            .post(format!("{}/v1/chat/completions", self.base_url))
            .json(&body)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;

        let content = resp
            .choices
            .first()
            .map(|c| c.message.content.as_str())
            .unwrap_or_default();
        parse_verdict(content)
            .ok_or_else(|| LlmError::Unparseable(content.chars().take(200).collect()))
    }
}

pub fn build_prompt(
    activity_name: &str,
    guidance: &str,
    conditions: &[Constraint],
    weather: &WeatherSnapshot,
) -> String {
    let mut prompt = format!(
        "Activity: {activity_name}\n\
         Guidance: {guidance}\n\
         Current weather at the location: temperature {:.1} C, wind {:.1} km/h, \
         precipitation {:.1} mm, visibility {:.1} km, {}.\n",
        weather.temperature_c,
        weather.wind_kmh,
        weather.precipitation_mm,
        weather.visibility_km,
        if weather.is_day {
            "daytime"
        } else {
            "nighttime"
        }
    );
    if !conditions.is_empty() {
        // The whole point (DESIGN.md D7): comparisons are computed here, in
        // code, so the model aggregates facts instead of doing arithmetic.
        // Grouped lists (rather than per-line MET/NOT MET annotations) keep a
        // small model from misparsing negations; "(none)" is unambiguous.
        let (met, unmet): (Vec<&Constraint>, Vec<&Constraint>) = conditions
            .iter()
            .partition(|c| constraint_satisfied(c, weather));
        let list = |v: &[&Constraint]| -> String {
            if v.is_empty() {
                "(none)".to_string()
            } else {
                v.iter()
                    .map(|c| c.description.as_str())
                    .collect::<Vec<_>>()
                    .join("; ")
            }
        };
        prompt.push_str(&format!(
            "Condition checks, already computed by the system (trust them):\n\
             Conditions that HOLD right now: {}\n\
             Conditions that do NOT hold: {}\n\
             Computed summary: {} of {} conditions hold right now.\n",
            list(&met),
            list(&unmet),
            met.len(),
            conditions.len()
        ));
    }
    prompt.push_str(
        "Apply the guidance to the computed checks and decide. \
         Answer with JSON only, reasoning first: {\"reasoning\": \"one or two sentences citing \
         the deciding conditions, ending with 'Conclusion: recommended.' or 'Conclusion: not \
         recommended.'\", \"recommended\": true or false}",
    );
    prompt
}

/// Defensive parse of LLM output: accepts a bare JSON object, or one embedded
/// in surrounding prose / markdown fences. `recommended` must be a real bool.
pub fn parse_verdict(text: &str) -> Option<LlmVerdict> {
    let candidates = [text.trim(), extract_json_object(text)?];
    for candidate in candidates {
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(candidate) {
            if let Some(recommended) = value.get("recommended").and_then(|v| v.as_bool()) {
                let reasoning = value
                    .get("reasoning")
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.trim().is_empty())
                    .unwrap_or("(no reasoning provided)")
                    .to_string();
                return Some(LlmVerdict {
                    recommended,
                    reasoning,
                });
            }
        }
    }
    None
}

fn extract_json_object(text: &str) -> Option<&str> {
    let start = text.find('{')?;
    let end = text.rfind('}')?;
    if end > start {
        Some(&text[start..=end])
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_clean_json() {
        let v = parse_verdict(r#"{"recommended": true, "reasoning": "Perfect beach weather."}"#)
            .unwrap();
        assert!(v.recommended);
        assert_eq!(v.reasoning, "Perfect beach weather.");
    }

    #[test]
    fn parses_json_inside_markdown_fence() {
        let text =
            "Here you go:\n```json\n{\"recommended\": false, \"reasoning\": \"Too windy.\"}\n```";
        let v = parse_verdict(text).unwrap();
        assert!(!v.recommended);
        assert_eq!(v.reasoning, "Too windy.");
    }

    #[test]
    fn missing_reasoning_gets_placeholder() {
        let v = parse_verdict(r#"{"recommended": true}"#).unwrap();
        assert_eq!(v.reasoning, "(no reasoning provided)");
    }

    #[test]
    fn rejects_non_boolean_recommended() {
        assert!(parse_verdict(r#"{"recommended": "yes", "reasoning": "x"}"#).is_none());
    }

    #[test]
    fn rejects_garbage() {
        assert!(parse_verdict("I think you should totally go!").is_none());
        assert!(parse_verdict("").is_none());
        assert!(parse_verdict("{broken json").is_none());
    }

    #[test]
    fn prompt_annotates_computed_checks() {
        use crate::weather::WeatherParam;
        let weather = WeatherSnapshot {
            temperature_c: 26.0,
            wind_kmh: 18.0,
            precipitation_mm: 0.0,
            visibility_km: 20.0,
            weather_code: 1,
            is_day: true,
        };
        let conditions = vec![
            Constraint {
                param: WeatherParam::TemperatureC,
                min: Some(22.0),
                max: Some(31.0),
                description: "Warm beach temperature".to_string(),
            },
            Constraint {
                param: WeatherParam::WindKmh,
                min: None,
                max: Some(12.0),
                description: "Calm enough for the ball".to_string(),
            },
            Constraint {
                param: WeatherParam::IsDay,
                min: None,
                max: Some(0.0),
                description: "It is nighttime".to_string(),
            },
        ];
        let prompt = build_prompt(
            "Matkot",
            "Recommend only if nothing is in the do-NOT-hold list.",
            &conditions,
            &weather,
        );
        assert!(prompt.contains("Guidance: Recommend only if nothing is in the do-NOT-hold list."));
        assert!(prompt.contains("Conditions that HOLD right now: Warm beach temperature"));
        // 18 > 12 and is_day -> 1.0, both computed in code:
        assert!(prompt
            .contains("Conditions that do NOT hold: Calm enough for the ball; It is nighttime"));
        assert!(prompt.contains("temperature 26.0 C"));
        assert!(prompt.contains("JSON only"));
    }

    #[test]
    fn prompt_without_conditions_has_no_checks_block() {
        let weather = WeatherSnapshot {
            temperature_c: 26.0,
            wind_kmh: 10.0,
            precipitation_mm: 0.0,
            visibility_km: 20.0,
            weather_code: 1,
            is_day: true,
        };
        let prompt = build_prompt("Matkot", "Best on warm calm days.", &[], &weather);
        assert!(prompt.contains("Guidance: Best on warm calm days."));
        assert!(!prompt.contains("Conditions that HOLD"));
        assert!(prompt.contains("JSON only"));
    }
}
