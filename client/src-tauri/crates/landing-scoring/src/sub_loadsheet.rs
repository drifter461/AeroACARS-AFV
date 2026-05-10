//! Loadsheet Sub-Score (NEU in v0.7.1).
//!
//! Spec docs/spec/v0.7.1-landing-ux-fairness.md F1: VFR-/Manual-Mode-
//! Piloten ohne Dispatch-Daten bekommen einen geskippten Loadsheet-Score
//! statt eines 0-Penalty. Master-Score weighted skipped raus.
//!
//! Erforderlich fuer Wertung: `planned_zfw_kg` UND `planned_tow_kg`
//! (beide kommen aus dem OFP). Wenn eines fehlt → skipped.
//!
//! Phase 2 implementiert die Skip-Logik. Die Score-Schwellen fuer
//! "zu schwer" / "out-of-balance" kommen erst in Phase 3 — die
//! Mass/Loadsheet-Bewertung war pre-v0.7.1 nicht in landingScoring.ts
//! enthalten und ist neu.
//!
//! Phase 2 Verhalten:
//!   - planned_zfw_kg None → skipped("no_planned_zfw")
//!   - planned_tow_kg None → skipped("no_planned_tow")
//!   - sonst: Score 100 (Placeholder bis Phase 3 Schwellen definiert
//!     sind — Loadsheet-Sub-Score existiert ab v0.7.1, gibt aber
//!     vorerst nur "ja/nein" als Wert).

use crate::{Band, SubScoreEntry};

pub fn sub_loadsheet(
    planned_zfw_kg: Option<f32>,
    planned_tow_kg: Option<f32>,
) -> SubScoreEntry {
    if planned_zfw_kg.is_none() {
        return SubScoreEntry::skipped(
            "loadsheet",
            "landing.sub.loadsheet",
            "no_planned_zfw",
        );
    }
    if planned_tow_kg.is_none() {
        return SubScoreEntry::skipped(
            "loadsheet",
            "landing.sub.loadsheet",
            "no_planned_tow",
        );
    }
    let zfw = planned_zfw_kg.unwrap();
    let tow = planned_tow_kg.unwrap();

    // Phase 2 Placeholder: Loadsheet vorhanden = Score 100. Phase 3
    // wird tatsaechliche Mass-Schwellen einfuehren (zu schwer, ueber
    // MTOW etc.) sobald die Backend-Daten fuer actual TOW/LDW
    // gegen planned vorhanden sind.
    let value = format!("ZFW {:.0} / TOW {:.0} kg", zfw, tow);
    SubScoreEntry::scored(
        "loadsheet",
        "landing.sub.loadsheet",
        100,
        value,
        "loadsheet_present",
        Band::Good,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skipped_when_no_zfw() {
        let s = sub_loadsheet(None, Some(70000.0));
        assert!(s.skipped);
        assert_eq!(s.reason.as_deref(), Some("no_planned_zfw"));
    }

    #[test]
    fn skipped_when_no_tow() {
        let s = sub_loadsheet(Some(50000.0), None);
        assert!(s.skipped);
        assert_eq!(s.reason.as_deref(), Some("no_planned_tow"));
    }

    #[test]
    fn scored_when_both_present() {
        let s = sub_loadsheet(Some(50000.0), Some(70000.0));
        assert!(!s.skipped);
        assert_eq!(s.score, 100);
        assert_eq!(s.value.as_deref(), Some("ZFW 50000 / TOW 70000 kg"));
    }
}
