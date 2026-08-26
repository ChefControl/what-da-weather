use std::collections::BTreeMap;

use anyhow::Context;
use serde::Deserialize;

use crate::weather::WeatherParam;

/// Top-level structure of `config/activities.yaml`.
/// `deny_unknown_fields` everywhere: typos fail startup instead of being ignored.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AppConfig {
    pub scheduler: SchedulerConfig,
    pub activities: BTreeMap<String, Activity>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SchedulerConfig {
    pub interval_minutes: u64,
    pub cities: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Activity {
    pub name: String,
    /// Free-text guidance sent to the LLM alongside the weather data. All
    /// preference nuance lives here; `required` only guards the impossible.
    pub prompt: String,
    /// Hard gate on daylight: the activity is only possible while the sun is
    /// up at the evaluated location (Open-Meteo's `is_day`).
    #[serde(default)]
    pub require_daylight: bool,
    #[serde(default)]
    pub required: Vec<Constraint>,
    /// Soft checks evaluated BY CODE; results are injected into the LLM prompt
    /// as pre-computed MET / NOT MET facts, so the model never does arithmetic.
    #[serde(default)]
    pub conditions: Vec<Constraint>,
    /// How the conditions aggregate; used to sanity-check the LLM's verdict.
    #[serde(default)]
    pub decision: DecisionPolicy,
}

/// How the soft conditions aggregate into the expected verdict.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DecisionPolicy {
    /// Recommended only when every condition holds (outdoor activities).
    #[default]
    All,
    /// Recommended when at least one condition holds (inverse preferences).
    Any,
}

/// A numeric bound on one weather parameter. `min`/`max` are inclusive; at
/// least one must be present.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Constraint {
    pub param: WeatherParam,
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub description: String,
}

impl Constraint {
    pub fn bounds_label(&self) -> String {
        match (self.min, self.max) {
            (Some(min), Some(max)) => format!("{min}..{max}"),
            (Some(min), None) => format!(">= {min}"),
            (None, Some(max)) => format!("<= {max}"),
            (None, None) => "unbounded".to_string(),
        }
    }
}

impl AppConfig {
    pub fn load(path: &str) -> anyhow::Result<Self> {
        let raw = std::fs::read_to_string(path)
            .with_context(|| format!("reading activities config at {path}"))?;
        let cfg: AppConfig =
            serde_yaml::from_str(&raw).with_context(|| format!("parsing {path}"))?;
        cfg.validate()
            .with_context(|| format!("validating {path}"))?;
        Ok(cfg)
    }

    fn validate(&self) -> anyhow::Result<()> {
        anyhow::ensure!(!self.activities.is_empty(), "no activities defined");
        anyhow::ensure!(
            !self.scheduler.cities.is_empty(),
            "scheduler.cities is empty"
        );
        anyhow::ensure!(
            self.scheduler.interval_minutes >= 1,
            "scheduler.interval_minutes must be >= 1"
        );
        for (key, activity) in &self.activities {
            anyhow::ensure!(
                !activity.prompt.trim().is_empty(),
                "activity '{key}': prompt must not be empty"
            );
            for c in activity.required.iter().chain(activity.conditions.iter()) {
                anyhow::ensure!(
                    c.min.is_some() || c.max.is_some(),
                    "activity '{key}': constraint on {:?} has neither min nor max",
                    c.param
                );
                if let (Some(min), Some(max)) = (c.min, c.max) {
                    anyhow::ensure!(
                        min <= max,
                        "activity '{key}': constraint on {:?} has min > max",
                        c.param
                    );
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_shipped_config() {
        // Keep the committed config honest: if config/activities.yaml drifts
        // from the schema, this test fails.
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../config/activities.yaml"
        );
        let cfg = AppConfig::load(path).expect("shipped config must parse");
        assert!(cfg.activities.contains_key("matkot"));
        assert!(cfg.activities.contains_key("gaming"));
        assert_eq!(cfg.scheduler.cities.len(), 3);
    }

    #[test]
    fn rejects_constraint_without_bounds() {
        let yaml = r#"
scheduler: { interval_minutes: 10, cities: ["X"] }
activities:
  a:
    name: "A"
    prompt: "test"
    required:
      - param: wind_kmh
        description: "no bounds"
"#;
        let cfg: AppConfig = serde_yaml::from_str(yaml).unwrap();
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn rejects_unknown_param() {
        let yaml = r#"
scheduler: { interval_minutes: 10, cities: ["X"] }
activities:
  a:
    name: "A"
    prompt: "test"
    required:
      - param: moon_phase
        max: 1
        description: "nope"
"#;
        assert!(serde_yaml::from_str::<AppConfig>(yaml).is_err());
    }

    #[test]
    fn rejects_empty_prompt() {
        let yaml = r#"
scheduler: { interval_minutes: 10, cities: ["X"] }
activities:
  a:
    name: "A"
    prompt: "  "
"#;
        let cfg: AppConfig = serde_yaml::from_str(yaml).unwrap();
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn rejects_unknown_field() {
        let yaml = r#"
scheduler: { interval_minutes: 10, cities: ["X"] }
activities:
  a:
    name: "A"
    prompt: "test"
    requird: []
"#;
        assert!(serde_yaml::from_str::<AppConfig>(yaml).is_err());
    }
}
