//! Rollout Sub-Score. 1:1-Port von TS `subRollout` in
//! landingScoring.ts:196-205. Returns `None` wenn rollout-Distanz
//! nicht verfuegbar ist (matched TS-Verhalten).

use crate::{Band, SubScoreEntry};

pub fn sub_rollout(rollout_m: Option<f32>) -> Option<SubScoreEntry> {
    let m = rollout_m?;
    let value = format!("{} m", m.round() as i32);

    let entry = if m < 800.0 {
        SubScoreEntry::scored(
            "rollout",
            "landing.sub.rollout",
            100,
            value,
            "excellent_stop",
            Band::Good,
        )
    } else if m < 1200.0 {
        SubScoreEntry::scored(
            "rollout",
            "landing.sub.rollout",
            80,
            value,
            "good_stop",
            Band::Good,
        )
    } else if m < 1800.0 {
        SubScoreEntry::scored(
            "rollout",
            "landing.sub.rollout",
            55,
            value,
            "long_rollout",
            Band::Ok,
        )
    } else if m < 2500.0 {
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

    fn run(m: f32) -> (u8, String) {
        let s = sub_rollout(Some(m)).unwrap();
        (s.points, s.rationale_key.unwrap())
    }

    #[test]
    fn none_returns_none() {
        assert!(sub_rollout(None).is_none());
    }

    #[test]
    fn ts_table_match() {
        assert_eq!(run(500.0), (100, "landing.rat.excellent_stop".into()));
        assert_eq!(run(799.99), (100, "landing.rat.excellent_stop".into()));
        assert_eq!(run(800.0), (80, "landing.rat.good_stop".into()));
        assert_eq!(run(1199.99), (80, "landing.rat.good_stop".into()));
        assert_eq!(run(1200.0), (55, "landing.rat.long_rollout".into()));
        assert_eq!(run(1799.99), (55, "landing.rat.long_rollout".into()));
        assert_eq!(run(1800.0), (25, "landing.rat.very_long_rollout".into()));
        assert_eq!(run(2499.99), (25, "landing.rat.very_long_rollout".into()));
        assert_eq!(run(2500.0), (5, "landing.rat.marginal_runway".into()));
    }
}
