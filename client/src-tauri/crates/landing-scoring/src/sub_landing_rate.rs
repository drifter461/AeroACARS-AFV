//! Landing-Rate Sub-Score. 1:1-Port von TS `subLandingRate` in
//! landingScoring.ts:144-159. Konstanten gespiegelt aus lib.rs
//! TOUCHDOWN_VS_*.

use crate::{Band, SubScoreEntry};

pub const T_VS_SMOOTH_FPM: f32 = 200.0;
pub const T_VS_FIRM_FPM: f32 = 400.0;
pub const T_VS_HARD_FPM: f32 = 600.0;
pub const T_VS_SEVERE_FPM: f32 = 1000.0;

pub fn sub_landing_rate(peak_vs_fpm: f32) -> SubScoreEntry {
    let vs = peak_vs_fpm.abs();
    let signed = peak_vs_fpm.round() as i32;
    let value = format!("{} fpm", if signed == 0 { 0 } else { signed });

    if vs < 60.0 {
        SubScoreEntry::scored(
            "landing_rate",
            "landing.sub.landing_rate",
            100,
            value,
            "smooth_touchdown",
            Band::Good,
        )
    } else if vs < T_VS_SMOOTH_FPM {
        SubScoreEntry::scored(
            "landing_rate",
            "landing.sub.landing_rate",
            90,
            value,
            "firm_but_clean",
            Band::Good,
        )
    } else if vs < T_VS_FIRM_FPM {
        SubScoreEntry::scored(
            "landing_rate",
            "landing.sub.landing_rate",
            70,
            value,
            "above_target",
            Band::Ok,
        )
    } else if vs < T_VS_HARD_FPM {
        SubScoreEntry::scored(
            "landing_rate",
            "landing.sub.landing_rate",
            45,
            value,
            "hard_landing",
            Band::Ok,
        )
    } else if vs < T_VS_SEVERE_FPM {
        SubScoreEntry::scored(
            "landing_rate",
            "landing.sub.landing_rate",
            20,
            value,
            "very_hard",
            Band::Bad,
        )
    } else {
        SubScoreEntry::scored(
            "landing_rate",
            "landing.sub.landing_rate",
            0,
            value,
            "severe_inspection",
            Band::Bad,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(vs: f32) -> (u8, &'static str) {
        let s = sub_landing_rate(vs);
        let r = match s.rationale_key.as_deref() {
            Some("landing.rat.smooth_touchdown") => "smooth_touchdown",
            Some("landing.rat.firm_but_clean") => "firm_but_clean",
            Some("landing.rat.above_target") => "above_target",
            Some("landing.rat.hard_landing") => "hard_landing",
            Some("landing.rat.very_hard") => "very_hard",
            Some("landing.rat.severe_inspection") => "severe_inspection",
            _ => "?",
        };
        (s.points, r)
    }

    #[test]
    fn ts_table_match() {
        // Aus landingScoring.ts:144-159 — vs.abs() Schwellen
        assert_eq!(run(-30.0), (100, "smooth_touchdown"));
        assert_eq!(run(-59.99), (100, "smooth_touchdown"));
        assert_eq!(run(-60.0), (90, "firm_but_clean"));
        assert_eq!(run(-199.99), (90, "firm_but_clean"));
        assert_eq!(run(-200.0), (70, "above_target"));
        assert_eq!(run(-399.99), (70, "above_target"));
        assert_eq!(run(-400.0), (45, "hard_landing"));
        assert_eq!(run(-599.99), (45, "hard_landing"));
        assert_eq!(run(-600.0), (20, "very_hard"));
        assert_eq!(run(-999.99), (20, "very_hard"));
        assert_eq!(run(-1000.0), (0, "severe_inspection"));
        assert_eq!(run(-2500.0), (0, "severe_inspection"));
    }

    #[test]
    fn value_format_matches_ts() {
        // TS rounded `signed` und schreibt `${signed} fpm`, 0 → "0 fpm"
        assert_eq!(sub_landing_rate(-191.4).value.unwrap(), "-191 fpm");
        assert_eq!(sub_landing_rate(0.0).value.unwrap(), "0 fpm");
        assert_eq!(sub_landing_rate(-413.7).value.unwrap(), "-414 fpm");
    }
}
