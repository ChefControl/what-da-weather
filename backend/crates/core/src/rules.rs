use crate::config::{Activity, Constraint};
use crate::weather::WeatherSnapshot;

#[derive(Debug, Clone)]
pub struct GateResult {
    pub passed: bool,
    pub failures: Vec<String>,
}

pub fn constraint_satisfied(c: &Constraint, w: &WeatherSnapshot) -> bool {
    let value = w.get(c.param);
    if let Some(min) = c.min {
        if value < min {
            return false;
        }
    }
    if let Some(max) = c.max {
        if value > max {
            return false;
        }
    }
    true
}

fn describe_violation(c: &Constraint, w: &WeatherSnapshot) -> String {
    format!(
        "{} ({} is {:.1}, allowed {})",
        c.description,
        c.param.label(),
        w.get(c.param),
        c.bounds_label()
    )
}

/// The hard gate (DESIGN.md §3): any violated required constraint blocks the
/// activity outright, and the LLM is not consulted. `require_daylight` gates
/// on Open-Meteo's location-aware `is_day` (sun position at the coordinates).
pub fn evaluate_gate(activity: &Activity, w: &WeatherSnapshot) -> GateResult {
    let mut failures: Vec<String> = Vec::new();
    if activity.require_daylight && !w.is_day {
        failures.push("Daylight required - the sun is down at this location".to_string());
    }
    failures.extend(
        activity
            .required
            .iter()
            .filter(|c| !constraint_satisfied(c, w))
            .map(|c| describe_violation(c, w)),
    );
    GateResult {
        passed: failures.is_empty(),
        failures,
    }
}

/// Rule-based substitute for the LLM verdict (DESIGN.md D6 fallback): the
/// activity is recommended when at least half of its preferred conditions hold.
pub fn fallback_verdict(preferred: &[Constraint], w: &WeatherSnapshot) -> (bool, String) {
    if preferred.is_empty() {
        return (
            true,
            "All hard constraints pass and no preferred conditions are defined.".to_string(),
        );
    }
    let (met, unmet): (Vec<&Constraint>, Vec<&Constraint>) =
        preferred.iter().partition(|c| constraint_satisfied(c, w));
    let recommended = met.len() * 2 >= preferred.len();
    let mut reasoning = format!(
        "Rule-based fallback: {}/{} preferred conditions met.",
        met.len(),
        preferred.len()
    );
    if !met.is_empty() {
        reasoning.push_str(&format!(
            " Met: {}.",
            met.iter()
                .map(|c| c.description.as_str())
                .collect::<Vec<_>>()
                .join("; ")
        ));
    }
    if !unmet.is_empty() {
        reasoning.push_str(&format!(
            " Unmet: {}.",
            unmet
                .iter()
                .map(|c| c.description.as_str())
                .collect::<Vec<_>>()
                .join("; ")
        ));
    }
    (recommended, reasoning)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::weather::WeatherParam;

    fn snapshot() -> WeatherSnapshot {
        WeatherSnapshot {
            temperature_c: 26.0,
            wind_kmh: 10.0,
            humidity_pct: 50.0,
            precipitation_mm: 0.0,
            cloud_cover_pct: 20.0,
            visibility_km: 20.0,
            weather_code: 1,
            is_day: true,
        }
    }

    fn constraint(param: WeatherParam, min: Option<f64>, max: Option<f64>) -> Constraint {
        Constraint {
            param,
            min,
            max,
            description: format!("{param:?} bound"),
        }
    }

    fn activity(required: Vec<Constraint>, require_daylight: bool) -> Activity {
        Activity {
            name: "Test".to_string(),
            require_daylight,
            required,
            preferred: vec![],
        }
    }

    #[test]
    fn gate_passes_when_all_required_hold() {
        let a = activity(
            vec![
                constraint(WeatherParam::WindKmh, None, Some(25.0)),
                constraint(WeatherParam::TemperatureC, Some(18.0), None),
            ],
            false,
        );
        let gate = evaluate_gate(&a, &snapshot());
        assert!(gate.passed);
        assert!(gate.failures.is_empty());
    }

    #[test]
    fn gate_reports_every_violation() {
        let mut w = snapshot();
        w.wind_kmh = 40.0;
        w.temperature_c = 10.0;
        let a = activity(
            vec![
                constraint(WeatherParam::WindKmh, None, Some(25.0)),
                constraint(WeatherParam::TemperatureC, Some(18.0), None),
                constraint(WeatherParam::PrecipitationMm, None, Some(0.5)),
            ],
            false,
        );
        let gate = evaluate_gate(&a, &w);
        assert!(!gate.passed);
        assert_eq!(gate.failures.len(), 2);
        assert!(gate.failures[0].contains("wind"));
    }

    #[test]
    fn daylight_gate_blocks_at_night() {
        let mut w = snapshot();
        w.is_day = false;
        let a = activity(
            vec![constraint(WeatherParam::WindKmh, None, Some(25.0))],
            true,
        );
        let gate = evaluate_gate(&a, &w);
        assert!(!gate.passed);
        assert_eq!(gate.failures.len(), 1);
        assert!(gate.failures[0].contains("Daylight required"));
    }

    #[test]
    fn daylight_gate_passes_during_the_day() {
        let a = activity(vec![], true);
        assert!(evaluate_gate(&a, &snapshot()).passed);
    }

    #[test]
    fn night_is_fine_without_daylight_requirement() {
        let mut w = snapshot();
        w.is_day = false;
        let a = activity(vec![], false);
        assert!(evaluate_gate(&a, &w).passed);
    }

    #[test]
    fn range_constraint_is_inclusive() {
        let c = constraint(WeatherParam::TemperatureC, Some(26.0), Some(26.0));
        assert!(constraint_satisfied(&c, &snapshot()));
    }

    #[test]
    fn fallback_recommends_at_half_or_more() {
        // 2 of 3 met -> recommended
        let preferred = vec![
            constraint(WeatherParam::TemperatureC, Some(23.0), Some(32.0)), // met (26)
            constraint(WeatherParam::CloudCoverPct, None, Some(40.0)),      // met (20)
            constraint(WeatherParam::HumidityPct, None, Some(40.0)),        // unmet (50)
        ];
        let (recommended, reasoning) = fallback_verdict(&preferred, &snapshot());
        assert!(recommended);
        assert!(reasoning.contains("2/3"));

        // 1 of 4 met -> not recommended (the "gaming on a nice day" case)
        let preferred = vec![
            constraint(WeatherParam::PrecipitationMm, Some(0.2), None), // unmet
            constraint(WeatherParam::WindKmh, Some(30.0), None),        // unmet
            constraint(WeatherParam::TemperatureC, None, Some(12.0)),   // unmet
            constraint(WeatherParam::CloudCoverPct, None, Some(90.0)),  // met
        ];
        let (recommended, _) = fallback_verdict(&preferred, &snapshot());
        assert!(!recommended);
    }

    #[test]
    fn fallback_with_no_preferences_recommends() {
        let (recommended, _) = fallback_verdict(&[], &snapshot());
        assert!(recommended);
    }
}
