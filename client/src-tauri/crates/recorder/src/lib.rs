//! Flight recorder — turns a `SimSnapshot` stream into:
//!
//!   * a chronological flight log (events: pushback, takeoff, gear, flaps, …)
//!   * a position history (for ACARS positions API + post-flight analysis)
//!   * a landing analysis bundle (runway ident, centerline deviation, heading deviation,
//!     threshold distance, bounces, METAR snapshot)
//!
//! See requirements spec §11, §13–§22.
//!
//! Status: Phase 1 stubs. Phase 2: full phase FSM. Phase 3: landing analyzer.

#![allow(dead_code)]

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sim_core::FlightPhase;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlightLogEvent {
    pub timestamp: DateTime<Utc>,
    pub kind: FlightLogEventKind,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FlightLogEventKind {
    AcarsStarted,
    PhpVmsConnected,
    SimulatorDetected,
    AircraftDetected,
    AircraftMismatch,
    FlightLoaded,
    PhaseChanged(FlightPhase),
    Takeoff,
    Touchdown,
    LandingRate,
    BounceDetected,
    PirepSubmitted,
    Other,
}

// TODO(phase-2): phase FSM
// TODO(phase-3): landing analyzer
