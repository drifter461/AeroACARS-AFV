//! Local persistence layer — SQLite (bundled, no system dep).
//!
//! Tables (Phase 1 baseline):
//!   * `outbound_queue` — pending API calls, replayed on reconnect
//!   * `flight_log`     — events with timestamps
//!   * `positions`      — high-rate position rows pending ACARS-positions submission
//!   * `settings`       — KVP cache of phpVMS-side config (TTL'd)
//!
//! See requirements spec §26.

#![allow(dead_code)]

use thiserror::Error;

#[derive(Debug, Error)]
pub enum StorageError {
    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

// TODO(phase-1): open/migrate the SQLite file under the OS-appropriate app-data dir
// (Win: %APPDATA%/CloudeAcars/cloudeacars.sqlite, macOS: ~/Library/Application Support/CloudeAcars/...).
