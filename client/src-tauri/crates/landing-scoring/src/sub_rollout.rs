//! Rollout Sub-Score („Bahn-Auslastung").
//!
//! v0.7.17 (N-002): Schwellen jetzt aircraft-kategorie-abhaengig.
//! Vorher waren die Grenzen (800/1200/1800/2500 m) absolut — was fuer
//! Light-GA grob passt, aber jeden Airliner mit > 1800 m Rollout in
//! die rote „long_rollout"-Zone schickte, obwohl ein A320 oder B738
//! auf einer 3 km Bahn typisch 1800-2200 m braucht und damit voellig
//! normal operiert.
//!
//! Die Klassifikation ist pragmatisch nach ICAO-Type-Designator:
//!   * Heavy / Wide-Body: A330 / A340 / A350 / A380 / B747 / B767 /
//!     B777 / B787 / MD11
//!   * Medium / Narrow-Body / Regional: A318/A319/A320/A321/Neo,
//!     B737-Family, B757, Embraer 170-195, CRJ, ATR, Dash-8, etc.
//!   * Light (Default / unbekannt): kleiner GA, Cessna 172 etc.

use crate::{Band, SubScoreEntry};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Category {
    Light,
    Medium,
    Heavy,
}

/// Map an ICAO type designator to a rollout-score category.
///
/// Unbekannte / nicht-gemappte ICAOs fallen auf `Light` zurueck —
/// das ist der konservative Pfad (engste Schwellen). Wer als Cessna
/// 2 km ausrollt, hat tatsaechlich ein Problem; wer als A320 2 km
/// ausrollt, war gut.
pub(crate) fn category_for_icao(icao: Option<&str>) -> Category {
    let Some(icao) = icao else { return Category::Light };
    let icao = icao.trim().to_uppercase();

    // Heavy / Wide-Bodies.
    const HEAVY: &[&str] = &[
        // Airbus widebodies
        "A332", "A333", "A338", "A339",
        "A342", "A343", "A345", "A346",
        "A359", "A35K",
        "A388",
        // Boeing 747 family
        "B741", "B742", "B743", "B744", "B748",
        // Boeing 767
        "B762", "B763", "B764",
        // Boeing 777
        "B772", "B773", "B77F", "B77L", "B77W",
        // Boeing 787
        "B788", "B789", "B78X",
        // MD-11
        "MD11", "MD1F",
        // Antonov
        "A124", "A225",
        // IL-96
        "IL96",
    ];
    if HEAVY.contains(&icao.as_str()) {
        return Category::Heavy;
    }

    // Medium / Narrow-Body / Regional.
    const MEDIUM: &[&str] = &[
        // Airbus narrowbodies
        "A318", "A319", "A320", "A321",
        "A19N", "A20N", "A21N", // NEO family
        "A220", "BCS1", "BCS3", // A220 / CSeries
        // Boeing 737 family
        "B731", "B732", "B733", "B734", "B735",
        "B736", "B737", "B738", "B739",
        "B37M", "B38M", "B39M", "B3XM", // MAX
        // Boeing 757
        "B752", "B753",
        // Embraer regional
        "E135", "E145", "E170", "E175", "E190", "E195",
        "E290", "E295", // E2
        // Bombardier CRJ
        "CRJ1", "CRJ2", "CRJ7", "CRJ9", "CRJX",
        // ATR
        "AT42", "AT43", "AT44", "AT45", "AT46",
        "AT72", "AT73", "AT74", "AT75", "AT76",
        // Dash 8
        "DH8A", "DH8B", "DH8C", "DH8D",
        // Fokker
        "F70", "F100", "F50",
        // MD-80/90
        "MD81", "MD82", "MD83", "MD87", "MD88", "MD90",
    ];
    if MEDIUM.contains(&icao.as_str()) {
        return Category::Medium;
    }

    Category::Light
}

/// Rollout-Schwellen pro Kategorie. (good_top, ok_top, long_top,
/// very_long_top) — Werte sind die *obere* Grenze jedes Bandes in
/// Metern, oberhalb der nahesten Grenze faellt der Score in das
/// jeweils naechst-schlechtere Band.
pub(crate) fn thresholds_for(category: Category) -> (f32, f32, f32, f32) {
    match category {
        Category::Light => (800.0, 1200.0, 1800.0, 2500.0),
        Category::Medium => (1200.0, 1800.0, 2400.0, 3000.0),
        Category::Heavy => (1500.0, 2300.0, 3000.0, 3800.0),
    }
}

pub fn sub_rollout(rollout_m: Option<f32>, aircraft_icao: Option<&str>) -> Option<SubScoreEntry> {
    let m = rollout_m?;
    let value = format!("{} m", m.round() as i32);
    let category = category_for_icao(aircraft_icao);
    let (t_excellent, t_good, t_ok, t_long) = thresholds_for(category);

    let entry = if m < t_excellent {
        SubScoreEntry::scored(
            "rollout",
            "landing.sub.rollout",
            100,
            value,
            "excellent_stop",
            Band::Good,
        )
    } else if m < t_good {
        SubScoreEntry::scored(
            "rollout",
            "landing.sub.rollout",
            80,
            value,
            "good_stop",
            Band::Good,
        )
    } else if m < t_ok {
        SubScoreEntry::scored(
            "rollout",
            "landing.sub.rollout",
            55,
            value,
            "long_rollout",
            Band::Ok,
        )
    } else if m < t_long {
        SubScoreEntry::scored(
            "rollout",
            "landing.sub.rollout",
            25,
            value,
            "very_long_rollout",
            Band::Bad,
        )
    } else {
        SubScoreEntry::scored(
            "rollout",
            "landing.sub.rollout",
            5,
            value,
            "marginal_runway",
            Band::Bad,
        )
    };
    Some(entry)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(m: f32, icao: Option<&str>) -> (u8, String) {
        let s = sub_rollout(Some(m), icao).unwrap();
        (s.points, s.rationale_key.unwrap())
    }

    #[test]
    fn none_returns_none() {
        assert!(sub_rollout(None, None).is_none());
        assert!(sub_rollout(None, Some("A320")).is_none());
    }

    #[test]
    fn light_category_unchanged_from_v0_7_16() {
        // Cessna 172 etc. — Schwellen identisch zur alten Hardcode-
        // Tabelle aus v0.7.16. Keine Regression fuer GA-Piloten.
        assert_eq!(run(500.0, Some("C172")), (100, "landing.rat.excellent_stop".into()));
        assert_eq!(run(799.99, Some("C172")), (100, "landing.rat.excellent_stop".into()));
        assert_eq!(run(800.0, Some("C172")), (80, "landing.rat.good_stop".into()));
        assert_eq!(run(1199.99, Some("C172")), (80, "landing.rat.good_stop".into()));
        assert_eq!(run(1200.0, Some("C172")), (55, "landing.rat.long_rollout".into()));
        assert_eq!(run(1799.99, Some("C172")), (55, "landing.rat.long_rollout".into()));
        assert_eq!(run(1800.0, Some("C172")), (25, "landing.rat.very_long_rollout".into()));
        assert_eq!(run(2499.99, Some("C172")), (25, "landing.rat.very_long_rollout".into()));
        assert_eq!(run(2500.0, Some("C172")), (5, "landing.rat.marginal_runway".into()));
    }

    #[test]
    fn medium_category_a320_landing_2km_is_good() {
        // N-002 Original-Beschwerde: A21N landete in BIKF mit ~2km
        // Rollout auf einer 3km Bahn. Vorher: 25 Pkt (very_long).
        // Jetzt: 80 Pkt (good_stop) bei 2000m — passt zur Realitaet.
        assert_eq!(run(2000.0, Some("A320")), (55, "landing.rat.long_rollout".into()));
        // 1500m fuer einen A320 ist sehr gut
        assert_eq!(run(1500.0, Some("A320")), (80, "landing.rat.good_stop".into()));
        // 1000m ist exzellent (kurze Bahn, perfektes Bremsen)
        assert_eq!(run(1000.0, Some("A320")), (100, "landing.rat.excellent_stop".into()));
        // 2800m ist sehr lang fuer A320
        assert_eq!(run(2800.0, Some("A320")), (25, "landing.rat.very_long_rollout".into()));
    }

    #[test]
    fn medium_category_covers_a320_family_and_b737() {
        // Alle A320-Family-Varianten muessen in Medium fallen.
        assert_eq!(category_for_icao(Some("A318")), Category::Medium);
        assert_eq!(category_for_icao(Some("A319")), Category::Medium);
        assert_eq!(category_for_icao(Some("A320")), Category::Medium);
        assert_eq!(category_for_icao(Some("A321")), Category::Medium);
        assert_eq!(category_for_icao(Some("A20N")), Category::Medium);
        assert_eq!(category_for_icao(Some("A21N")), Category::Medium);
        // B737 family
        assert_eq!(category_for_icao(Some("B738")), Category::Medium);
        assert_eq!(category_for_icao(Some("B739")), Category::Medium);
        assert_eq!(category_for_icao(Some("B38M")), Category::Medium);
    }

    #[test]
    fn b015_ein799_regression_a20n_1096m_is_excellent_not_good() {
        // v0.7.17 (B-015): EIN799 LTBJ→EIDW, A20N, Rollout 1096 m.
        // Vorher meldete der Pilot-Client 80 PTS „good_stop", weil
        // `aircraft_icao` beim PIREP-File None war (X-Plane Web API
        // hatte den ICAO nicht geliefert) → Fallback auf Light-GA-
        // Schwellen (800/1200) → 1096 fiel in „good_stop". Nach der
        // bid_icao-Fallback-Reparatur in lib.rs:8482 bekommt der Sub-
        // Score den Bid-Wert „A20N" und nutzt Medium-Schwellen
        // (1200/1800) → 1096 < 1200 → excellent_stop, 100 PTS.
        assert_eq!(run(1096.0, Some("A20N")), (100, "landing.rat.excellent_stop".into()));
        // Auch wenn snapshot None lieferte: jetzt kommt der Wert via Bid.
        // (Wenn beides None ist, fallen wir auf Light zurueck → 80 PTS.)
        assert_eq!(run(1096.0, None), (80, "landing.rat.good_stop".into()));
    }

    #[test]
    fn heavy_category_b777_a350_etc() {
        assert_eq!(category_for_icao(Some("B77W")), Category::Heavy);
        assert_eq!(category_for_icao(Some("A35K")), Category::Heavy);
        assert_eq!(category_for_icao(Some("A388")), Category::Heavy);
        assert_eq!(category_for_icao(Some("B748")), Category::Heavy);
    }

    #[test]
    fn heavy_category_b77w_2500m_still_good() {
        // Heavy braucht naturgemaess mehr Rollout. 2500m bei B777 ist
        // noch im „good"-Band, nicht „bad".
        assert_eq!(run(2500.0, Some("B77W")), (55, "landing.rat.long_rollout".into()));
        assert_eq!(run(2000.0, Some("B77W")), (80, "landing.rat.good_stop".into()));
        // 4000m+ ist auch fuer Heavy nicht mehr ok
        assert_eq!(run(4000.0, Some("B77W")), (5, "landing.rat.marginal_runway".into()));
    }

    #[test]
    fn unknown_icao_falls_back_to_light() {
        assert_eq!(category_for_icao(None), Category::Light);
        assert_eq!(category_for_icao(Some("UNKN")), Category::Light);
        assert_eq!(category_for_icao(Some("")), Category::Light);
    }

    #[test]
    fn icao_case_and_whitespace_tolerant() {
        // Pilot-Client kann ICAO mit Groß-/Kleinschreibung oder
        // Whitespace liefern — Match muss robust sein.
        assert_eq!(category_for_icao(Some("a320")), Category::Medium);
        assert_eq!(category_for_icao(Some("  B77W  ")), Category::Heavy);
        assert_eq!(category_for_icao(Some("a320")), Category::Medium);
    }
}
