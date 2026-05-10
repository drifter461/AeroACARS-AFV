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

use crate::{Band, SubScoreEntry};

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
}
