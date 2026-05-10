//! G-Force Sub-Score. 1:1-Port von TS `subGForce` in
//! landingScoring.ts:161-168.

use crate::{Band, SubScoreEntry};

pub const T_G_SMOOTH: f32 = 1.20;
pub const T_G_FIRM: f32 = 1.40;
pub const T_G_HARD: f32 = 1.70;
pub const T_G_SEVERE: f32 = 2.10;

pub fn sub_g_force(peak_g: f32) -> SubScoreEntry {
    let value = format!("{:.2} G", peak_g);

    if peak_g < T_G_SMOOTH {
        SubScoreEntry::scored("g_force", "landing.sub.g_force", 100, value, "smooth_g", Band::Good)
    } else if peak_g < T_G_FIRM {
        SubScoreEntry::scored(
            "g_force",
            "landing.sub.g_force",
            85,
            value,
            "comfortable_g",
            Band::Good,
        )
    } else if peak_g < T_G_HARD {
        SubScoreEntry::scored(
            "g_force",
            "landing.sub.g_force",
            60,
            value,
            "noticeable_g",
            Band::Ok,
        )
    } else if peak_g < T_G_SEVERE {
        SubScoreEntry::scored("g_force", "landing.sub.g_force", 30, value, "firm_g", Band::Bad)
    } else {
        SubScoreEntry::scored("g_force", "landing.sub.g_force", 0, value, "severe_g", Band::Bad)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(g: f32) -> (u8, String) {
        let s = sub_g_force(g);
        (s.points, s.rationale_key.unwrap())
    }

    #[test]
    fn ts_table_match() {
        // landingScoring.ts:161-168
        assert_eq!(run(1.0), (100, "landing.rat.smooth_g".into()));
        assert_eq!(run(1.19), (100, "landing.rat.smooth_g".into()));
        assert_eq!(run(1.20), (85, "landing.rat.comfortable_g".into()));
        assert_eq!(run(1.39), (85, "landing.rat.comfortable_g".into()));
        assert_eq!(run(1.40), (60, "landing.rat.noticeable_g".into()));
        assert_eq!(run(1.69), (60, "landing.rat.noticeable_g".into()));
        assert_eq!(run(1.70), (30, "landing.rat.firm_g".into()));
        assert_eq!(run(2.09), (30, "landing.rat.firm_g".into()));
        assert_eq!(run(2.10), (0, "landing.rat.severe_g".into()));
        assert_eq!(run(3.5), (0, "landing.rat.severe_g".into()));
    }

    #[test]
    fn value_format_matches_ts() {
        assert_eq!(sub_g_force(1.32).value.unwrap(), "1.32 G");
        assert_eq!(sub_g_force(1.0).value.unwrap(), "1.00 G");
    }
}
