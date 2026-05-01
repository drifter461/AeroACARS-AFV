//! METAR fetch + parse.
//!
//! Snapshots weather at the moment of takeoff and touchdown for the PIREP landing analysis.
//! See requirements spec §21–§22.
//!
//! Source TBD (open architecture question A2): NOAA aviationweather.gov (free, rate-limited)
//! is the default candidate. CheckWX / AVWX are paid alternatives.
//!
//! Status: Phase 3 stub.

#![allow(dead_code)]

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetarSnapshot {
    pub icao: String,
    pub raw: String,
    pub time: DateTime<Utc>,
    pub wind_direction_deg: Option<f32>,
    pub wind_speed_kt: Option<f32>,
    pub gust_kt: Option<f32>,
    pub visibility_m: Option<u32>,
    pub temperature_c: Option<f32>,
    pub dewpoint_c: Option<f32>,
    pub qnh_hpa: Option<f32>,
}
