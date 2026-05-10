//! v0.7.5 Phase-Safety Hotfix Replay-Tests.
//!
//! Spec: docs/spec/flight-phase-state-machine.md §13.8 + §13.9 + §16.
//!
//! Zwei reale Bug-Klassen, durch echte VPS-Pilot-Daten belegt:
//!   1. URO913: Universal Arrived-Fallback while rolling (engines=0 + gs > 1)
//!   2. PTO105: Holding-Pending leakt phasenuebergreifend (5.2s Hold trotz 90s Dwell)
//!
//! Tests pruefen:
//! - **Helper-Logik** (arrived_fallback_conditions_basic + should_reset_holding_pending)
//!   gegen die kritischen Bedingungen aus den Real-Logs
//! - **Fixture-Daten** sind echt geladen + zeigen die Bug-Symptome
//!   die in den Real-Logs gefunden wurden
//!
//! Anonymisierte Fixtures aus tests/fixtures/phase_*.jsonl.gz:
//!   - phase_uro913_arrived_fallback_rolling.jsonl.gz
//!   - phase_pto105_holding_pending_leak.jsonl.gz
//!   - phase_dlh742_valid_holding.jsonl.gz (positiv-Beleg fuer §13.9)

use aeroacars_app_lib::{
    arrived_fallback_conditions_basic, should_reset_holding_pending, PublicSimKind,
};
use flate2::read::GzDecoder;
use sim_core::FlightPhase;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;

// Quiet unused-warning: we re-use this for fixture aircraft sanity if needed
#[allow(dead_code)]
fn _unused_marker(_: PublicSimKind) {}

// ─── Fixture-Pfade ───────────────────────────────────────────────────────

fn fixture_path(name: &str) -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.push("tests");
    p.push("fixtures");
    p.push(name);
    p
}

fn read_jsonl_gz(path: &std::path::Path) -> Vec<serde_json::Value> {
    let f = File::open(path).expect("fixture file");
    let gz = GzDecoder::new(f);
    let r = BufReader::new(gz);
    r.lines()
        .filter_map(|l| l.ok())
        .filter_map(|l| serde_json::from_str::<serde_json::Value>(&l).ok())
        .collect()
}

// ─── Helper-Tests (echte Code-Fix-Verifikation) ──────────────────────────

#[test]
fn arrived_fallback_blocks_when_rolling() {
    // URO913-Klasse: on_ground + engines=0 ABER gs > 1 → Fallback BLOCKIERT
    assert!(!arrived_fallback_conditions_basic(true, 0, 42.0));
    assert!(!arrived_fallback_conditions_basic(true, 0, 141.0));
    assert!(!arrived_fallback_conditions_basic(true, 0, 1.5));
}

#[test]
fn arrived_fallback_fires_when_truly_stationary() {
    // Echtes Stillstand: gs < 1 → Fallback ARMS
    assert!(arrived_fallback_conditions_basic(true, 0, 0.0));
    assert!(arrived_fallback_conditions_basic(true, 0, 0.5));
    assert!(arrived_fallback_conditions_basic(true, 0, 0.99));
}

#[test]
fn arrived_fallback_blocks_when_airborne() {
    // Sanity: niemals Fallback wenn nicht on_ground
    assert!(!arrived_fallback_conditions_basic(false, 0, 0.0));
    assert!(!arrived_fallback_conditions_basic(false, 0, 100.0));
}

#[test]
fn arrived_fallback_blocks_when_engines_running() {
    // Sanity: niemals Fallback wenn Engines an
    assert!(!arrived_fallback_conditions_basic(true, 1, 0.0));
    assert!(!arrived_fallback_conditions_basic(true, 2, 0.0));
}

#[test]
fn holding_pending_resets_on_phase_exit() {
    // PTO105-Klasse: Phase wechselt von X zu non-Holding → reset
    assert!(should_reset_holding_pending(
        FlightPhase::Approach,
        FlightPhase::Final
    ));
    assert!(should_reset_holding_pending(
        FlightPhase::Cruise,
        FlightPhase::Descent
    ));
    assert!(should_reset_holding_pending(
        FlightPhase::Approach,
        FlightPhase::Climb
    ));
    assert!(should_reset_holding_pending(
        FlightPhase::Final,
        FlightPhase::Landing
    ));
}

#[test]
fn holding_pending_kept_on_holding_entry() {
    // Wenn echt zu Holding gewechselt wird → NICHT resetten
    // (der Pending-Counter ist genau dafuer da)
    assert!(!should_reset_holding_pending(
        FlightPhase::Cruise,
        FlightPhase::Holding
    ));
    assert!(!should_reset_holding_pending(
        FlightPhase::Approach,
        FlightPhase::Holding
    ));
}

#[test]
fn holding_pending_no_reset_on_no_transition() {
    // Wenn keine echte Transition (gleiche Phase) → kein Reset
    assert!(!should_reset_holding_pending(
        FlightPhase::Cruise,
        FlightPhase::Cruise
    ));
    assert!(!should_reset_holding_pending(
        FlightPhase::Holding,
        FlightPhase::Holding
    ));
}

// ─── Fixture-Replay-Tests (Daten-Verifikation) ──────────────────────────

#[test]
fn fixture_uro913_shows_engines_off_while_rolling() {
    // Lade die anonymisierte URO913-Sequenz und verifiziere dass die kritischen
    // Snapshots (on_ground=true + engines_running=0 + groundspeed > 1) wirklich da sind.
    // Damit ist sichergestellt: der Real-Bug existiert in den Fixture-Daten und der
    // Code-Fix (arrived_fallback_conditions_basic mit gs<1) wuerde ihn jetzt blocken.
    let events = read_jsonl_gz(&fixture_path(
        "phase_uro913_arrived_fallback_rolling.jsonl.gz",
    ));
    assert!(!events.is_empty(), "fixture leer");

    let rolling_with_engines_off: Vec<_> = events
        .iter()
        .filter(|e| e["type"] == "position")
        .filter_map(|e| {
            let s = &e["snapshot"];
            let on_ground = s["on_ground"].as_bool()?;
            let engines = s["engines_running"].as_u64()?;
            let gs = s["groundspeed_kt"].as_f64()?;
            if on_ground && engines == 0 && gs >= 1.0 {
                Some(gs)
            } else {
                None
            }
        })
        .collect();

    assert!(
        !rolling_with_engines_off.is_empty(),
        "URO913 fixture sollte Snapshots mit on_ground=true + engines=0 + gs>=1 \
         enthalten (Bug-Symptom)"
    );

    // Mit dem Fix wuerde arrived_fallback_conditions_basic fuer ALLE diese
    // Snapshots false liefern → Fallback bleibt aus.
    for gs in &rolling_with_engines_off {
        assert!(
            !arrived_fallback_conditions_basic(true, 0, *gs as f32),
            "Fix muss alle rolling-Snapshots blocken: gs={}",
            gs
        );
    }
}

#[test]
fn fixture_pto105_shows_short_holding_episode() {
    // Lade PTO105-Fixture und verifiziere: Approach -> Holding -> Approach
    // mit ungewoehnlich kurzer Holding-Dauer (< 90s, der erwarteten Dwell).
    // Das ist das Bug-Symptom — echtes Holding muesste >= 90s sein.
    let events = read_jsonl_gz(&fixture_path(
        "phase_pto105_holding_pending_leak.jsonl.gz",
    ));
    assert!(!events.is_empty(), "PTO105 fixture leer");

    let holding_entries: Vec<_> = events
        .iter()
        .filter(|e| e["type"] == "phase_changed" && e["to"] == "Holding")
        .collect();
    let holding_exits: Vec<_> = events
        .iter()
        .filter(|e| e["type"] == "phase_changed" && e["from"] == "Holding")
        .collect();

    assert!(
        !holding_entries.is_empty(),
        "PTO105 fixture muss mindestens 1 Holding-Entry zeigen"
    );
    assert!(
        !holding_exits.is_empty(),
        "PTO105 fixture muss mindestens 1 Holding-Exit zeigen"
    );

    // Pruefe dass mindestens eine Holding-Episode kuerzer als 90s war
    use chrono::DateTime;
    let entry_ts = DateTime::parse_from_rfc3339(
        holding_entries[0]["timestamp"].as_str().unwrap(),
    )
    .unwrap();
    let exit_ts = DateTime::parse_from_rfc3339(
        holding_exits[0]["timestamp"].as_str().unwrap(),
    )
    .unwrap();
    let duration_secs = (exit_ts - entry_ts).num_seconds();
    assert!(
        duration_secs < 90,
        "PTO105 fixture sollte Holding < 90s zeigen (Bug-Symptom — \
         pending-leak schlaegt zu frueh zu). Tatsaechlich: {} s",
        duration_secs
    );
    assert!(
        duration_secs > 0,
        "Holding-Episode-Dauer sollte positiv sein"
    );

    // Mit Fix wird beim Phase-Wechsel Approach -> Final/Landing/Climb der
    // holding_pending_since reset → naechster Approach-Wechsel kann nicht
    // sofort wieder Holding triggern.
    assert!(should_reset_holding_pending(
        FlightPhase::Approach,
        FlightPhase::Final
    ));
}

#[test]
fn fixture_dlh742_valid_holding_episode() {
    // Positiv-Beleg: DLH742 hat ein ECHTES Holding (~109s) das gewuenscht
    // erkannt werden soll. Mit dem Fix darf das nicht beschaedigt werden.
    let events = read_jsonl_gz(&fixture_path("phase_dlh742_valid_holding.jsonl.gz"));
    assert!(!events.is_empty(), "DLH742 fixture leer");

    let holding_entries: Vec<_> = events
        .iter()
        .filter(|e| e["type"] == "phase_changed" && e["to"] == "Holding")
        .collect();
    let holding_exits: Vec<_> = events
        .iter()
        .filter(|e| e["type"] == "phase_changed" && e["from"] == "Holding")
        .collect();

    assert!(!holding_entries.is_empty(), "DLH742 muss Holding zeigen");
    assert!(!holding_exits.is_empty(), "DLH742 muss Holding-Exit zeigen");

    // Pruefe dass die Holding-Episode "echt" war (>= 90s Dwell)
    use chrono::DateTime;
    let entry_ts = DateTime::parse_from_rfc3339(
        holding_entries[0]["timestamp"].as_str().unwrap(),
    )
    .unwrap();
    let exit_ts = DateTime::parse_from_rfc3339(
        holding_exits[0]["timestamp"].as_str().unwrap(),
    )
    .unwrap();
    let duration_secs = (exit_ts - entry_ts).num_seconds();
    assert!(
        duration_secs >= 90,
        "DLH742 Holding muss >= 90s sein (echter Hold). Tatsaechlich: {}",
        duration_secs
    );

    // Mit dem Fix muss `should_reset_holding_pending` korrekt FALSE
    // liefern wenn Cruise -> Holding gewechselt wird (kein Reset)
    assert!(!should_reset_holding_pending(
        FlightPhase::Cruise,
        FlightPhase::Holding
    ));
}
