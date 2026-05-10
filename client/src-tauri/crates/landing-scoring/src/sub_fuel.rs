//! Fuel Sub-Score.
//!
//! Phase 0 (jetzt): 1:1-Port von TS `subFuel` in landingScoring.ts:207-216.
//!   - Symmetrische Schwelle (Math.abs(efficiency)).
//!   - Returns `None` wenn efficiency_pct nicht verfuegbar ist.
//!
//! Phase 2/F2+F3 (spaeter): wird durch `sub_fuel_v0_7_1` ersetzt mit
//!   - Hard-Gate: kein planned_burn → skipped (kein Fallback)
//!   - Asymmetrie: Minderverbrauch nicht bestrafen
//!   - Label-Wechsel "Spritverbrauch" → "OFP-Treue"
//!
//! Phase 0 behaelt die Legacy-Funktion fuer Goldenset-Tests.
//! Phase 2 fuegt `sub_fuel_v0_7_1` hinzu — wird ab v0.7.1 verwendet.

use crate::{Band, SubScoreEntry};

/// v0.7.1 Phase 2 (F2 + F3): Fuel-Score mit Hard-Gate + Asymmetrie.
///
/// F2: kein planned_burn → skipped (KEIN Fallback)
///     kein actual_trip_burn → skipped
/// F3: efficiency = (actual - planned) / planned * 100
///     Mehrverbrauch (efficiency > 0): wie Legacy bestraft
///     Minderverbrauch (efficiency <= 0): nicht bestraft, ggf. Bonus
///     Starker Minderverbrauch (>15% under): Warning "planned_burn_may_be_off"
/// Label-Aenderung: "Spritverbrauch" → "OFP-Treue" (i18n key bleibt
/// `landing.sub.fuel`, der String dahinter aendert sich in Phase 3).
pub fn sub_fuel_v0_7_1(
    planned_burn_kg: Option<f32>,
    actual_trip_burn_kg: Option<f32>,
) -> SubScoreEntry {
    let Some(planned) = planned_burn_kg else {
        return SubScoreEntry::skipped("fuel", "landing.sub.fuel", "no_planned_burn");
    };
    if planned <= 0.0 {
        return SubScoreEntry::skipped("fuel", "landing.sub.fuel", "no_planned_burn");
    }
    let Some(actual) = actual_trip_burn_kg else {
        return SubScoreEntry::skipped("fuel", "landing.sub.fuel", "no_actual_burn");
    };

    let efficiency = ((actual - planned) / planned) * 100.0;
    let value = if efficiency > 0.0 {
        format!("+{:.1}%", efficiency)
    } else {
        format!("{:.1}%", efficiency)
    };

    if efficiency > 0.0 {
        // Mehrverbrauch — score-relevant wie Legacy
        if efficiency < 2.0 {
            SubScoreEntry::scored(
                "fuel",
                "landing.sub.fuel",
                100,
                value,
                "on_plan",
                Band::Good,
            )
        } else if efficiency < 5.0 {
            SubScoreEntry::scored(
                "fuel",
                "landing.sub.fuel",
                80,
                value,
                "near_plan",
                Band::Good,
            )
        } else if efficiency < 10.0 {
            SubScoreEntry::scored(
                "fuel",
                "landing.sub.fuel",
                55,
                value,
                "off_plan",
                Band::Ok,
            )
        } else if efficiency < 20.0 {
            SubScoreEntry::scored(
                "fuel",
                "landing.sub.fuel",
                25,
                value,
                "very_off_plan",
                Band::Bad,
            )
        } else {
            SubScoreEntry::scored(
                "fuel",
                "landing.sub.fuel",
                5,
                value,
                "way_off_plan",
                Band::Bad,
            )
        }
    } else {
        // Minderverbrauch (efficiency <= 0) — KEIN Penalty
        let under = efficiency.abs();
        if under < 5.0 {
            SubScoreEntry::scored(
                "fuel",
                "landing.sub.fuel",
                100,
                value,
                "on_plan",
                Band::Good,
            )
        } else if under < 15.0 {
            SubScoreEntry::scored(
                "fuel",
                "landing.sub.fuel",
                95,
                value,
                "efficient",
                Band::Good,
            )
        } else {
            // > 15% under → score 85, plus warning
            let mut entry = SubScoreEntry::scored(
                "fuel",
                "landing.sub.fuel",
                85,
                value,
                "very_efficient",
                Band::Good,
            );
            entry.warning = Some("planned_burn_may_be_off".to_string());
            entry
        }
    }
}

pub fn sub_fuel_legacy(efficiency_pct: Option<f32>) -> Option<SubScoreEntry> {
    let pct = efficiency_pct?;
    let dev = pct.abs();
    let value = if pct > 0.0 {
        format!("+{:.1}%", pct)
    } else {
        format!("{:.1}%", pct)
    };

    let entry = if dev < 2.0 {
        SubScoreEntry::scored("fuel", "landing.sub.fuel", 100, value, "on_plan", Band::Good)
    } else if dev < 5.0 {
        SubScoreEntry::scored("fuel", "landing.sub.fuel", 80, value, "near_plan", Band::Good)
    } else if dev < 10.0 {
        SubScoreEntry::scored("fuel", "landing.sub.fuel", 55, value, "off_plan", Band::Ok)
    } else if dev < 20.0 {
        SubScoreEntry::scored("fuel", "landing.sub.fuel", 25, value, "very_off_plan", Band::Bad)
    } else {
        SubScoreEntry::scored("fuel", "landing.sub.fuel", 5, value, "way_off_plan", Band::Bad)
    };
    Some(entry)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(pct: f32) -> (u8, String) {
        let s = sub_fuel_legacy(Some(pct)).unwrap();
        (s.points, s.rationale_key.unwrap())
    }

    #[test]
    fn none_returns_none() {
        assert!(sub_fuel_legacy(None).is_none());
    }

    #[test]
    fn ts_table_match_symmetric() {
        // Phase-0 Legacy: Math.abs → -5% gleich +5%
        assert_eq!(run(0.0), (100, "landing.rat.on_plan".into()));
        assert_eq!(run(1.99), (100, "landing.rat.on_plan".into()));
        assert_eq!(run(-1.99), (100, "landing.rat.on_plan".into()));
        assert_eq!(run(2.0), (80, "landing.rat.near_plan".into()));
        assert_eq!(run(-2.0), (80, "landing.rat.near_plan".into()));
        assert_eq!(run(4.99), (80, "landing.rat.near_plan".into()));
        assert_eq!(run(5.0), (55, "landing.rat.off_plan".into()));
        assert_eq!(run(-7.5), (55, "landing.rat.off_plan".into()));
        assert_eq!(run(10.0), (25, "landing.rat.very_off_plan".into()));
        assert_eq!(run(-15.0), (25, "landing.rat.very_off_plan".into()));
        assert_eq!(run(20.0), (5, "landing.rat.way_off_plan".into()));
        assert_eq!(run(-30.0), (5, "landing.rat.way_off_plan".into()));
    }

    #[test]
    fn value_format_matches_ts() {
        assert_eq!(sub_fuel_legacy(Some(5.2)).unwrap().value.unwrap(), "+5.2%");
        assert_eq!(sub_fuel_legacy(Some(-5.2)).unwrap().value.unwrap(), "-5.2%");
        assert_eq!(sub_fuel_legacy(Some(0.0)).unwrap().value.unwrap(), "0.0%");
    }

    // ─── v0.7.1 sub_fuel_v0_7_1 (F2 Hard-Gate + F3 Asymmetrie) ────────

    #[test]
    fn v0_7_1_hard_gate_no_planned() {
        let s = sub_fuel_v0_7_1(None, Some(5000.0));
        assert!(s.skipped);
        assert_eq!(s.reason.as_deref(), Some("no_planned_burn"));
        assert_eq!(s.score, 0); // skipped → 0 — wird via aggregate ignoriert
    }

    #[test]
    fn v0_7_1_hard_gate_no_actual() {
        let s = sub_fuel_v0_7_1(Some(5000.0), None);
        assert!(s.skipped);
        assert_eq!(s.reason.as_deref(), Some("no_actual_burn"));
    }

    #[test]
    fn v0_7_1_hard_gate_zero_planned() {
        let s = sub_fuel_v0_7_1(Some(0.0), Some(5000.0));
        assert!(s.skipped);
        assert_eq!(s.reason.as_deref(), Some("no_planned_burn"));
    }

    #[test]
    fn v0_7_1_overburn_punished() {
        // +5% Mehrverbrauch → 55 (off_plan)
        let s = sub_fuel_v0_7_1(Some(5000.0), Some(5350.0)); // +7%
        assert_eq!(s.score, 55);
        assert_eq!(s.rationale_key.as_deref(), Some("landing.rat.off_plan"));
        assert!(s.warning.is_none());
    }

    #[test]
    fn v0_7_1_underburn_not_punished() {
        // -10% Minderverbrauch → 95 (efficient), KEIN Warning
        let s = sub_fuel_v0_7_1(Some(5000.0), Some(4500.0));
        assert_eq!(s.score, 95);
        assert_eq!(s.rationale_key.as_deref(), Some("landing.rat.efficient"));
        assert!(s.warning.is_none());
    }

    #[test]
    fn v0_7_1_strong_underburn_warns() {
        // -25% Minderverbrauch → 85, plus Warning planned_burn_may_be_off
        let s = sub_fuel_v0_7_1(Some(5000.0), Some(3750.0));
        assert_eq!(s.score, 85);
        assert_eq!(s.rationale_key.as_deref(), Some("landing.rat.very_efficient"));
        assert_eq!(s.warning.as_deref(), Some("planned_burn_may_be_off"));
    }

    #[test]
    fn v0_7_1_on_plan() {
        // Exact match → 100 (on_plan)
        let s = sub_fuel_v0_7_1(Some(5000.0), Some(5000.0));
        assert_eq!(s.score, 100);
        assert_eq!(s.rationale_key.as_deref(), Some("landing.rat.on_plan"));
    }
}
