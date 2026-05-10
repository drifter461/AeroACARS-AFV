//! Phase-0-Goldenset: Sub-Score-Werte muessen bit-identisch zu
//! `client/src/lib/landingScoring.ts` sein.
//!
//! Diese Fixtures sind aus den 6 v0.7.0-Replay-Fluegen + synthetischen
//! Edge-Cases. Wenn ein Wert hier abweicht, ist die Rust-Implementation
//! gegenueber TS gedriftet — Phase-0-Acceptance-Gate (Spec §7.1):
//! Drift > 0.5 Punkte blockiert weitere Phasen.
//!
//! Die TS-Vergleichswerte wurden manuell aus landingScoring.ts:
//! computeSubScores + aggregateSubScores berechnet und sind als
//! Kommentar dokumentiert.

use landing_scoring::{aggregate_master_score, compute_sub_scores, LandingScoringInput};

/// Hilfsmethode: extrahiert Sub-Score-Punkte fuer den gegebenen key
/// oder paniked wenn nicht gefunden.
fn pts(subs: &[landing_scoring::SubScoreEntry], key: &str) -> u8 {
    subs.iter()
        .find(|s| s.key == key)
        .unwrap_or_else(|| panic!("sub-score '{}' fehlt: {:?}", key, subs))
        .score
}

/// Hilfsmethode: stellt sicher dass kein Sub-Score mit dem Key vorhanden
/// ist (Phase 0: nur fuel/stability/rollout sind optional).
fn no_sub(subs: &[landing_scoring::SubScoreEntry], key: &str) {
    assert!(
        !subs.iter().any(|s| s.key == key),
        "sub-score '{}' sollte nicht vorhanden sein: {:?}",
        key,
        subs
    );
}

// ─── Replay-Fluege (Spec §7.2.1 Backward-Compat-Tabelle) ───────────

#[test]
fn pto105_msfs_smooth_55fpm() {
    // Pilot-Daten: vs=-55 fpm, peak_g=1.10 G, bounces=0,
    //   approach_vs_stddev=80 fpm, approach_bank_stddev=2.5°,
    //   rollout=900m, fuel_efficiency=+1.0%
    // TS-Erwartung:
    //   landing_rate(55) → vs<60 → 100 (smooth_touchdown)
    //   g_force(1.10)    → <T_G_SMOOTH=1.20 → 100 (smooth_g)
    //   bounces(0)       → 100 (clean_set)
    //   stability(80, 2.5) → vs_band(80<100)=100 / bk_band(2.5<5)=80 → min=80 (stable)
    //   rollout(900)     → 800<m<1200 → 80 (good_stop)
    //   fuel(+1.0%)      → abs<2 → 100 (on_plan)
    // Master = (100*3 + 100*3 + 100*2 + 80*2 + 80*1 + 100*1) / (3+3+2+2+1+1)
    //        = (300+300+200+160+80+100) / 12 = 1140/12 = 95
    let input = LandingScoringInput {
        vs_fpm: Some(-55.0),
        peak_g_load: Some(1.10),
        bounce_count: Some(0),
        approach_vs_stddev_fpm: Some(80.0),
        approach_bank_stddev_deg: Some(2.5),
        rollout_distance_m: Some(900.0),
        fuel_efficiency_pct: Some(1.0),
        ..Default::default()
    };
    let subs = compute_sub_scores(&input);
    assert_eq!(pts(&subs, "landing_rate"), 100);
    assert_eq!(pts(&subs, "g_force"), 100);
    assert_eq!(pts(&subs, "bounces"), 100);
    assert_eq!(pts(&subs, "stability"), 80);
    assert_eq!(pts(&subs, "rollout"), 80);
    assert_eq!(pts(&subs, "fuel"), 100);
    assert_eq!(aggregate_master_score(&subs), Some(95));
}

#[test]
fn dlh304_msfs_acceptable_357fpm() {
    // Pilot-Daten: vs=-357 fpm, peak_g=1.45 G, bounces=0,
    //   approach_vs_stddev=180 fpm, approach_bank_stddev=4.0°,
    //   rollout=1500m, fuel=-3.5%
    // TS-Erwartung:
    //   landing_rate(357) → 200<vs<400 → 70 (above_target)
    //   g_force(1.45)    → 1.40<g<1.70 → 60 (noticeable_g)
    //   bounces(0)       → 100
    //   stability(180, 4.0) → vs(180<200)=80 / bk(4<5)=80 → min=80 (stable)
    //   rollout(1500)    → 1200<m<1800 → 55 (long_rollout)
    //   fuel(-3.5)       → abs<5 → 80 (near_plan)
    // Master = (70*3+60*3+100*2+80*2+55+80) / 12 = (210+180+200+160+55+80) / 12
    //        = 885/12 = 73.75 → round → 74
    let input = LandingScoringInput {
        vs_fpm: Some(-357.0),
        peak_g_load: Some(1.45),
        bounce_count: Some(0),
        approach_vs_stddev_fpm: Some(180.0),
        approach_bank_stddev_deg: Some(4.0),
        rollout_distance_m: Some(1500.0),
        fuel_efficiency_pct: Some(-3.5),
        ..Default::default()
    };
    let subs = compute_sub_scores(&input);
    assert_eq!(pts(&subs, "landing_rate"), 70);
    assert_eq!(pts(&subs, "g_force"), 60);
    assert_eq!(pts(&subs, "bounces"), 100);
    assert_eq!(pts(&subs, "stability"), 80);
    assert_eq!(pts(&subs, "rollout"), 55);
    assert_eq!(pts(&subs, "fuel"), 80);
    assert_eq!(aggregate_master_score(&subs), Some(74));
}

#[test]
fn cfg785_msfs_smooth_142fpm() {
    // vs=-142 fpm, peak_g=1.18, bounces=0, stab vs=70/bank=2.0, rollout=750m, fuel=+0.5
    // landing_rate(142) → 60<=vs<200 → 90 (firm_but_clean)
    // g(1.18) → <1.20 → 100 (smooth_g)
    // bounces(0) → 100
    // stability(70, 2.0) → vs=100, bk(2.0 nicht <2 → 80) → min=80
    // rollout(750) → <800 → 100
    // fuel(+0.5) → <2 → 100
    // Master = (90*3+100*3+100*2+80*2+100+100) / 12
    //        = (270+300+200+160+100+100) / 12 = 1130/12 = 94.166 → 94
    let input = LandingScoringInput {
        vs_fpm: Some(-142.0),
        peak_g_load: Some(1.18),
        bounce_count: Some(0),
        approach_vs_stddev_fpm: Some(70.0),
        approach_bank_stddev_deg: Some(2.0),
        rollout_distance_m: Some(750.0),
        fuel_efficiency_pct: Some(0.5),
        ..Default::default()
    };
    let subs = compute_sub_scores(&input);
    assert_eq!(pts(&subs, "landing_rate"), 90);
    assert_eq!(pts(&subs, "g_force"), 100);
    assert_eq!(pts(&subs, "bounces"), 100);
    assert_eq!(pts(&subs, "stability"), 80);
    assert_eq!(pts(&subs, "rollout"), 100);
    assert_eq!(pts(&subs, "fuel"), 100);
    assert_eq!(aggregate_master_score(&subs), Some(94));
}

#[test]
fn dah3181_xplane_firm_414fpm() {
    // v0.7.0 Fix: vs=-414 fpm (war +104 fpm Bug)
    // peak_g=1.55, bounces=0, stab vs=120/bank=3.0, rollout=1700m, fuel=+8.0
    // landing_rate(414) → 400<vs<600 → 45 (hard_landing)
    // g(1.55) → 1.40<g<1.70 → 60 (noticeable_g)
    // bounces(0) → 100
    // stability(120, 3.0) → vs(120<200)=80 / bk(3<5)=80 → 80
    // rollout(1700) → 1200<m<1800 → 55
    // fuel(+8.0) → 5<dev<10 → 55 (off_plan)
    // Master = (45*3+60*3+100*2+80*2+55+55) / 12
    //        = (135+180+200+160+55+55) / 12 = 785/12 = 65.4166 → 65
    let input = LandingScoringInput {
        vs_fpm: Some(-414.0),
        peak_g_load: Some(1.55),
        bounce_count: Some(0),
        approach_vs_stddev_fpm: Some(120.0),
        approach_bank_stddev_deg: Some(3.0),
        rollout_distance_m: Some(1700.0),
        fuel_efficiency_pct: Some(8.0),
        ..Default::default()
    };
    let subs = compute_sub_scores(&input);
    assert_eq!(pts(&subs, "landing_rate"), 45);
    assert_eq!(pts(&subs, "g_force"), 60);
    assert_eq!(pts(&subs, "bounces"), 100);
    assert_eq!(pts(&subs, "stability"), 80);
    assert_eq!(pts(&subs, "rollout"), 55);
    assert_eq!(pts(&subs, "fuel"), 55);
    assert_eq!(aggregate_master_score(&subs), Some(65));
}

// ─── Edge-Cases (synthetisch) ───────────────────────────────────────

#[test]
fn vfr_kein_fuel_kein_stability() {
    // Phase-0-Verhalten: fehlende stability-Felder + None fuel
    // → diese Sub-Scores fehlen, Bounces bleibt da (default 0)
    let input = LandingScoringInput {
        vs_fpm: Some(-200.0),
        peak_g_load: Some(1.30),
        bounce_count: None, // → wird 0
        approach_vs_stddev_fpm: None,
        approach_bank_stddev_deg: None,
        rollout_distance_m: None,
        fuel_efficiency_pct: None,
        ..Default::default()
    };
    let subs = compute_sub_scores(&input);
    assert_eq!(pts(&subs, "landing_rate"), 70);
    // g_force(1.30): 1.20<=g<1.40 → 85 (comfortable_g), NICHT noticeable_g
    assert_eq!(pts(&subs, "g_force"), 85);
    assert_eq!(pts(&subs, "bounces"), 100); // default 0 → clean_set
    no_sub(&subs, "stability");
    no_sub(&subs, "rollout");
    no_sub(&subs, "fuel");
    // Master = (70*3+85*3+100*2) / (3+3+2) = (210+255+200) / 8 = 665/8 = 83.125 → 83
    assert_eq!(aggregate_master_score(&subs), Some(83));
}

#[test]
fn empty_input_returns_only_bounces() {
    // Kein VS, kein G, alle anderen None → nur bounces (default 0)
    let input = LandingScoringInput::default();
    let subs = compute_sub_scores(&input);
    assert_eq!(subs.len(), 1);
    assert_eq!(subs[0].key, "bounces");
    assert_eq!(subs[0].score, 100);
    assert_eq!(aggregate_master_score(&subs), Some(100));
}
