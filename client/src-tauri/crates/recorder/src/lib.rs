//! Flight recorder — append-only JSONL log per flight.
//!
//! Captures a chronological event stream (phase transitions, position
//! samples, activity-log items) plus the final analyzer bundle when
//! the PIREP is filed. Files live under
//! `<app_data_dir>/flight_logs/<pirep_id>.jsonl` so each flight is a
//! self-contained replay artifact: copy/paste it into a debugger,
//! diff two flights, or feed it back into the FSM offline.
//!
//! Format: one JSON object per line, written via append-mode `O_APPEND`
//! so concurrent writers (we don't have any today, but future replay
//! agents might) can't tear a row.
//!
//! See requirements spec §11, §13–§22.

#![allow(dead_code)]

use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sim_core::{FlightPhase, SimSnapshot};
use thiserror::Error;

const LOGS_SUBDIR: &str = "flight_logs";

#[derive(Debug, Error)]
pub enum RecorderError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("serialization error: {0}")]
    Serde(#[from] serde_json::Error),
}

/// Tagged union of everything we write into the per-flight log. New
/// variants get added as the FSM and analyzers grow — the JSONL format
/// is forward-compatible because each row is self-describing.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum FlightLogEvent {
    /// Flight was started (fresh prefile or adopt). Captures the
    /// route + airline so a viewer doesn't need a sidecar manifest.
    FlightStarted {
        timestamp: DateTime<Utc>,
        pirep_id: String,
        airline_icao: String,
        flight_number: String,
        dpt_airport: String,
        arr_airport: String,
    },
    /// Flight resumed after a Tauri restart.
    FlightResumed {
        timestamp: DateTime<Utc>,
        pirep_id: String,
        age_minutes: i64,
    },
    /// Phase-FSM transitioned. Recorded once per change so post-hoc
    /// you can see exactly when boarding ended, takeoff fired, etc.
    PhaseChanged {
        timestamp: DateTime<Utc>,
        from: FlightPhase,
        to: FlightPhase,
        altitude_msl_ft: f64,
        groundspeed_kt: f32,
        altitude_agl_ft: f64,
    },
    /// Per-tick position snapshot. The full SimSnapshot is embedded so
    /// downstream tooling (offline analyzer, replay viewer) has every
    /// telemetry value the streamer saw at that moment.
    Position {
        timestamp: DateTime<Utc>,
        snapshot: SimSnapshot,
    },
    /// Activity-log entry (squawk change, lights toggle, AP engage,
    /// METAR fetch, …) — same string the user sees in the dashboard.
    Activity {
        timestamp: DateTime<Utc>,
        level: String,
        message: String,
        detail: Option<String>,
    },
    /// Touchdown analyzer settled — final score with the contributing
    /// peak values. Mirrors the LandingScore enum in lib.rs.
    LandingScored {
        timestamp: DateTime<Utc>,
        score: String,
        peak_vs_fpm: f32,
        peak_g_force: f32,
        bounce_count: u8,
    },
    /// PIREP filed (clean or manual) or cancelled. Closes the log.
    FlightEnded {
        timestamp: DateTime<Utc>,
        pirep_id: String,
        outcome: FlightOutcome,
    },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FlightOutcome {
    Filed,
    Manual,
    Cancelled,
    Forgotten,
}

/// Append-only writer for one flight's log. Cheap to construct — just
/// holds a path. Each `append` opens / appends / closes so a Tauri
/// crash never leaves a half-written line.
pub struct FlightRecorder {
    path: PathBuf,
}

impl FlightRecorder {
    /// Open (or implicitly create) the log file for this PIREP under
    /// `<app_data_dir>/flight_logs/<pirep_id>.jsonl`. The PIREP id is
    /// path-sanitised so a malicious server can't traverse the FS.
    pub fn open(app_data_dir: impl AsRef<Path>, pirep_id: &str) -> Result<Self, RecorderError> {
        let dir = app_data_dir.as_ref().join(LOGS_SUBDIR);
        std::fs::create_dir_all(&dir)?;
        let safe = sanitize_pirep_id(pirep_id);
        Ok(Self {
            path: dir.join(format!("{safe}.jsonl")),
        })
    }

    /// Append one event as a JSON line. Best-effort — errors are
    /// returned to the caller but the recorder is intended to be
    /// fire-and-forget from the streamer's perspective.
    pub fn append(&self, event: &FlightLogEvent) -> Result<(), RecorderError> {
        let mut line = serde_json::to_vec(event)?;
        line.push(b'\n');
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        file.write_all(&line)?;
        Ok(())
    }

    /// Path to the underlying file. Useful for the dashboard's "open
    /// flight log folder" button (future) or for a `Show in Explorer`
    /// helper.
    pub fn path(&self) -> &Path {
        &self.path
    }
}

/// Strip anything that isn't a safe filename character. PIREP ids are
/// always alphanumeric in practice, but harden against `..`/`/` if a
/// future phpVMS deployment changes the format.
fn sanitize_pirep_id(raw: &str) -> String {
    raw.chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect()
}

/// Aggregate stats across all per-flight log files under
/// `<app_data_dir>/flight_logs/`. Used by the Settings → Storage panel
/// to show "X Logs · Y MB belegen" before the user clicks delete.
#[derive(Debug, Clone, Copy, Default, Serialize)]
pub struct FlightLogStats {
    pub count: u32,
    pub total_bytes: u64,
}

pub fn flight_logs_stats(app_data_dir: impl AsRef<Path>) -> Result<FlightLogStats, RecorderError> {
    let dir = app_data_dir.as_ref().join(LOGS_SUBDIR);
    if !dir.exists() {
        return Ok(FlightLogStats::default());
    }
    let mut stats = FlightLogStats::default();
    for entry in std::fs::read_dir(&dir)? {
        let entry = entry?;
        let meta = entry.metadata()?;
        if meta.is_file() && entry.path().extension().and_then(|e| e.to_str()) == Some("jsonl") {
            stats.count += 1;
            stats.total_bytes += meta.len();
        }
    }
    Ok(stats)
}

/// Delete every `*.jsonl` under `<app_data_dir>/flight_logs/`. Returns
/// the count of files actually removed (best-effort — read errors on
/// individual files are skipped, not reported).
pub fn flight_logs_delete_all(app_data_dir: impl AsRef<Path>) -> Result<u32, RecorderError> {
    let dir = app_data_dir.as_ref().join(LOGS_SUBDIR);
    if !dir.exists() {
        return Ok(0);
    }
    let mut removed = 0u32;
    for entry in std::fs::read_dir(&dir)? {
        let Ok(entry) = entry else { continue };
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("jsonl") {
            if std::fs::remove_file(&path).is_ok() {
                removed += 1;
            }
        }
    }
    Ok(removed)
}

/// Delete `*.jsonl` files whose mtime is older than `older_than_days`.
/// Used by the Settings auto-purge toggle (default 30 days). Returns
/// the count of files removed. Files whose mtime can't be read are
/// left alone.
pub fn flight_logs_purge_older_than(
    app_data_dir: impl AsRef<Path>,
    older_than_days: u32,
) -> Result<u32, RecorderError> {
    let dir = app_data_dir.as_ref().join(LOGS_SUBDIR);
    if !dir.exists() {
        return Ok(0);
    }
    let cutoff = std::time::SystemTime::now()
        .checked_sub(std::time::Duration::from_secs(u64::from(older_than_days) * 86_400))
        .unwrap_or(std::time::UNIX_EPOCH);
    let mut removed = 0u32;
    for entry in std::fs::read_dir(&dir)? {
        let Ok(entry) = entry else { continue };
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
            continue;
        }
        let Ok(meta) = entry.metadata() else { continue };
        let Ok(mtime) = meta.modified() else { continue };
        if mtime < cutoff && std::fs::remove_file(&path).is_ok() {
            removed += 1;
        }
    }
    Ok(removed)
}
