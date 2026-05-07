//! Landing-score sub-component computation. **Single source of truth.**
//!
//! Pre-v0.5.23 the scoring math lived twice:
//!   1. `client/src/components/LandingPanel.tsx` — for the in-app PIREP detail
//!   2. `aeroacars-live/webapp/src/components/landingScoring.ts` — for the
//!      live monitor's Landing-Analyse modal
//!
//! That duplication produced different numbers for the same flight and
//! cost the user trust ("warum müssen wir das neu erfinden und nutzen
//! nicht die Berechnung"). v0.5.23 collapses it: the Rust adapter
//! computes the sub-scores once, ships them as part of the MQTT
//! `touchdown` payload, the live monitor renders them. The TS math
//! files become pure label lookups.
//!
//! Schwellwerte 1:1 wie die ursprüngliche LandingPanel.tsx-Implementation
//! — wenn man hier eine Schwelle ändert, ändert sich gleichzeitig der
//! In-App-PIREP UND die Live-Monitor-Auswertung.

use serde::{Deserialize, Serialize};

/// Visual-tone band for a sub-score. Drives card colour + bar gradient.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Band {
    Good,
    Ok,
    Bad,
}

impl Band {
    fn from_points(p: i32) -> Self {
        if p >= 85 {
            Band::Good
        } else if p >= 65 {
            Band::Ok
        } else {
            Band::Bad
        }
    }
}

/// Identifies which dimension of the landing this score covers. The
/// server (and the in-app PIREP) maps the key to a localized label.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SubScoreKey {
    LandingRate,
    GForce,
    Bounces,
    Stability,
    Rollout,
    Fuel,
}

/// One sub-score row as it appears in the breakdown UI.
///
/// `value` is pre-formatted (`"-53 fpm"`, `"1.07 G"`, …) so renderers
/// don't reimplement number-formatting. `rationale` is a stable string
/// key that the renderer translates to a localized label
/// ("smooth_touchdown" → "Butterweich aufgesetzt") via its i18n table.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubScore {
    pub key: SubScoreKey,
    pub points: i32,
    pub value: String,
    pub band: Band,
    pub rationale: &'static str,
}

// ─── Scoring functions — port of the TS originals ──────────────────

fn score_landing_rate(fpm: f32) -> (i32, &'static str) {
    let abs = fpm.abs();
    if abs <= 100.0 {
        (100, "smooth_touchdown")
    } else if abs <= 240.0 {
        (90, "firm_but_clean")
    } else if abs <= 360.0 {
        (75, "above_target")
    } else if abs <= 600.0 {
        (50, "hard_landing")
    } else if abs <= 1000.0 {
        (25, "very_hard")
    } else {
        (0, "severe_inspection")
    }
}

fn score_g_force(g: f32) -> (i32, &'static str) {
    if g <= 1.2 {
        (100, "smooth_g")
    } else if g <= 1.4 {
        (95, "comfortable_g")
    } else if g <= 1.7 {
        (80, "noticeable_g")
    } else if g <= 2.0 {
        (60, "firm_g")
    } else if g <= 2.6 {
        (30, "hard_g")
    } else {
        (0, "severe_g")
    }
}

fn score_bounces(n: u32) -> (i32, &'static str) {
    match n {
        0 => (100, "clean_set"),
        1 => (65, "one_bounce"),
        2 => (35, "two_bounces"),
        _ => (0, "many_bounces"),
    }
}

fn score_stability(vs_std: f32) -> (i32, &'static str) {
    if vs_std <= 100.0 {
        (100, "very_stable")
    } else if vs_std <= 200.0 {
        (85, "stable")
    } else if vs_std <= 300.0 {
        (65, "average_stability")
    } else if vs_std <= 400.0 {
        (45, "unstable_approach")
    } else {
        (25, "very_unstable")
    }
}

fn score_rollout(used_pct: f32) -> (i32, &'static str) {
    if used_pct <= 50.0 {
        (100, "excellent_stop")
    } else if used_pct <= 70.0 {
        (85, "good_stop")
    } else if used_pct <= 85.0 {
        (65, "long_rollout")
    } else if used_pct <= 95.0 {
        (40, "very_long_rollout")
    } else {
        (20, "marginal_runway")
    }
}

fn score_fuel(abs_pct: f32) -> (i32, &'static str) {
    if abs_pct <= 5.0 {
        (100, "on_plan")
    } else if abs_pct <= 10.0 {
        (85, "near_plan")
    } else if abs_pct <= 20.0 {
        (65, "off_plan")
    } else if abs_pct <= 30.0 {
        (45, "very_off_plan")
    } else {
        (25, "way_off_plan")
    }
}

/// Inputs the breakdown computation needs. Caller plucks them out of
/// the active `FlightStats` snapshot at touchdown time.
#[derive(Debug, Default, Clone)]
pub struct ScoringInputs {
    pub vs_fpm: Option<f32>,
    /// Peak G during the touchdown window. Falls back to `g_load` when None.
    pub peak_g_force: Option<f32>,
    pub g_force: Option<f32>,
    pub bounce_count: Option<u32>,
    pub approach_vs_stddev_fpm: Option<f32>,
    pub rollout_distance_m: Option<f32>,
    /// Total runway length in metres. Required for the rollout sub-score.
    pub runway_length_m: Option<f32>,
    /// Fuel efficiency pct = (actual_burn − planned_burn) / planned_burn × 100.
    pub fuel_efficiency_pct: Option<f32>,
}

/// Build the sub-score list. Suppresses any sub-score whose required
/// input isn't available rather than guessing.
pub fn compute_sub_scores(r: &ScoringInputs) -> Vec<SubScore> {
    let mut out: Vec<SubScore> = Vec::new();

    if let Some(vs) = r.vs_fpm {
        let (points, rationale) = score_landing_rate(vs);
        out.push(SubScore {
            key: SubScoreKey::LandingRate,
            points,
            value: format!("{:.0} fpm", vs),
            band: Band::from_points(points),
            rationale,
        });
    }

    let g = r.peak_g_force.or(r.g_force);
    if let Some(g) = g {
        let (points, rationale) = score_g_force(g);
        out.push(SubScore {
            key: SubScoreKey::GForce,
            points,
            value: format!("{:.2} G", g),
            band: Band::from_points(points),
            rationale,
        });
    }

    if let Some(n) = r.bounce_count {
        let (points, rationale) = score_bounces(n);
        out.push(SubScore {
            key: SubScoreKey::Bounces,
            points,
            value: n.to_string(),
            band: Band::from_points(points),
            rationale,
        });
    }

    if let Some(sigma) = r.approach_vs_stddev_fpm {
        let (points, rationale) = score_stability(sigma);
        out.push(SubScore {
            key: SubScoreKey::Stability,
            points,
            value: format!("{:.0} fpm σ", sigma),
            band: Band::from_points(points),
            rationale,
        });
    }

    if let (Some(rollout), Some(length)) = (r.rollout_distance_m, r.runway_length_m) {
        if length > 0.0 {
            let used_pct = (rollout / length) * 100.0;
            let (points, rationale) = score_rollout(used_pct);
            out.push(SubScore {
                key: SubScoreKey::Rollout,
                points,
                value: format!("{:.0}% ({:.0} m)", used_pct, rollout),
                band: Band::from_points(points),
                rationale,
            });
        }
    }

    if let Some(pct) = r.fuel_efficiency_pct {
        let (points, rationale) = score_fuel(pct.abs());
        let sign = if pct >= 0.0 { "+" } else { "" };
        out.push(SubScore {
            key: SubScoreKey::Fuel,
            points,
            value: format!("{}{:.1}%", sign, pct),
            band: Band::from_points(points),
            rationale,
        });
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smooth_touchdown_scores_full_marks() {
        let r = ScoringInputs {
            vs_fpm: Some(-53.0),
            peak_g_force: Some(1.07),
            bounce_count: Some(0),
            ..Default::default()
        };
        let subs = compute_sub_scores(&r);
        assert_eq!(subs.len(), 3);
        assert_eq!(subs[0].points, 100); // landing-rate
        assert_eq!(subs[0].rationale, "smooth_touchdown");
        assert_eq!(subs[1].points, 100); // g-force
        assert_eq!(subs[1].rationale, "smooth_g");
        assert_eq!(subs[2].points, 100); // bounces
    }

    #[test]
    fn very_unstable_approach_scored_25() {
        let r = ScoringInputs {
            vs_fpm: Some(-50.0),
            peak_g_force: Some(1.07),
            bounce_count: Some(0),
            approach_vs_stddev_fpm: Some(585.0),
            ..Default::default()
        };
        let subs = compute_sub_scores(&r);
        let stability = subs.iter().find(|s| s.key == SubScoreKey::Stability).unwrap();
        assert_eq!(stability.points, 25);
        assert_eq!(stability.rationale, "very_unstable");
    }

    #[test]
    fn rollout_suppressed_without_runway_length() {
        let r = ScoringInputs {
            rollout_distance_m: Some(1747.0),
            runway_length_m: None,
            ..Default::default()
        };
        let subs = compute_sub_scores(&r);
        assert!(!subs.iter().any(|s| s.key == SubScoreKey::Rollout));
    }

    #[test]
    fn rollout_at_58pct_scores_solider_bremsweg() {
        let r = ScoringInputs {
            rollout_distance_m: Some(1747.0),
            runway_length_m: Some(3000.0),
            ..Default::default()
        };
        let subs = compute_sub_scores(&r);
        let rollout = subs.iter().find(|s| s.key == SubScoreKey::Rollout).unwrap();
        assert_eq!(rollout.points, 85); // 58% → 85 PTS, "good_stop"
        assert_eq!(rollout.rationale, "good_stop");
        assert_eq!(rollout.value, "58% (1747 m)");
    }

    #[test]
    fn fuel_minus_4_7pct_on_plan() {
        let r = ScoringInputs {
            fuel_efficiency_pct: Some(-4.7),
            ..Default::default()
        };
        let subs = compute_sub_scores(&r);
        let fuel = subs.iter().find(|s| s.key == SubScoreKey::Fuel).unwrap();
        assert_eq!(fuel.points, 100);
        assert_eq!(fuel.rationale, "on_plan");
        assert_eq!(fuel.value, "-4.7%");
    }
}
