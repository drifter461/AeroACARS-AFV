//! Static SimVar list + byte-level parser for the data block
//! SimConnect sends back per `SIMCONNECT_RECV_SIMOBJECT_DATA`.
//!
//! Each entry in [`TELEMETRY_FIELDS`] is added in order to the data
//! definition; the parser reads the same order at fixed offsets. The
//! whole point of this module is that **a single rejected SimVar can
//! never shift another field's position** — every field knows its
//! width and we walk the buffer step by step. If a SimVar is rejected
//! by SimConnect, the data block is shorter than expected and `parse`
//! either returns the value or [`f64::NAN`] / `0` for the missing
//! tail; nothing prior shifts.

use chrono::Utc;
use sim_core::{AircraftProfile, SimSnapshot, Simulator};

const KG_PER_LB: f64 = 0.453_592_37;

#[derive(Debug, Clone, Copy)]
pub enum FieldKind {
    /// 8-byte IEEE 754.
    Float64,
    /// 4-byte signed integer (SimConnect bool is INT32).
    Int32,
    /// 256-byte fixed buffer, NUL-terminated.
    String256,
}

impl FieldKind {
    pub fn size(self) -> usize {
        match self {
            FieldKind::Float64 => 8,
            FieldKind::Int32 => 4,
            FieldKind::String256 => 256,
        }
    }
}

/// Static description of one telemetry field.
#[derive(Debug, Clone, Copy)]
pub struct TelemetryField {
    pub name: &'static str,
    pub unit: &'static str,
    pub kind: FieldKind,
}

/// Order matters: this is exactly the order in which SimConnect will
/// pack the bytes for us.
pub const TELEMETRY_FIELDS: &[TelemetryField] = &[
    // ---- Identity ----
    F::str("TITLE", ""),
    F::str("ATC MODEL", ""),
    F::str("ATC ID", ""),
    // ---- Position ----
    F::f64("PLANE LATITUDE", "degrees"),
    F::f64("PLANE LONGITUDE", "degrees"),
    F::f64("PLANE ALTITUDE", "feet"),
    F::f64("PLANE ALT ABOVE GROUND", "feet"),
    // ---- Attitude / motion ----
    F::f64("PLANE HEADING DEGREES TRUE", "degrees"),
    F::f64("PLANE HEADING DEGREES MAGNETIC", "degrees"),
    F::f64("PLANE PITCH DEGREES", "degrees"),
    F::f64("PLANE BANK DEGREES", "degrees"),
    F::f64("VERTICAL SPEED", "feet per minute"),
    // ---- Speeds ----
    F::f64("GROUND VELOCITY", "knots"),
    F::f64("AIRSPEED INDICATED", "knots"),
    F::f64("AIRSPEED TRUE", "knots"),
    F::f64("G FORCE", "GForce"),
    // ---- Aircraft state ----
    F::bool("SIM ON GROUND"),
    F::bool("BRAKE PARKING POSITION"),
    F::bool("STALL WARNING"),
    F::bool("OVERSPEED WARNING"),
    F::f64("GEAR POSITION", "percent over 100"),
    F::f64("FLAPS HANDLE PERCENT", "percent over 100"),
    F::bool("GENERAL ENG COMBUSTION:1"),
    F::bool("GENERAL ENG COMBUSTION:2"),
    F::bool("GENERAL ENG COMBUSTION:3"),
    F::bool("GENERAL ENG COMBUSTION:4"),
    // ---- Fuel & weight (SU2 EX1 + legacy fallback) ----
    F::f64("FUEL TOTAL QUANTITY WEIGHT EX1", "pounds"),
    F::f64("FUEL TOTAL QUANTITY WEIGHT", "pounds"),
    F::f64("TOTAL WEIGHT", "pounds"),
    F::f64("EMPTY WEIGHT", "pounds"),
    // ---- Environment ----
    F::f64("AMBIENT WIND DIRECTION", "degrees"),
    F::f64("AMBIENT WIND VELOCITY", "knots"),
    F::f64("KOHLSMAN SETTING MB", "millibars"),
    F::f64("AMBIENT TEMPERATURE", "celsius"),
];

// Helper builders so the table above stays compact.
struct F;
impl F {
    const fn str(name: &'static str, unit: &'static str) -> TelemetryField {
        TelemetryField {
            name,
            unit,
            kind: FieldKind::String256,
        }
    }
    const fn f64(name: &'static str, unit: &'static str) -> TelemetryField {
        TelemetryField {
            name,
            unit,
            kind: FieldKind::Float64,
        }
    }
    const fn bool(name: &'static str) -> TelemetryField {
        TelemetryField {
            name,
            unit: "bool",
            kind: FieldKind::Int32,
        }
    }
}

/// Decoded telemetry — one snapshot's worth of values, before the
/// final mapping into [`SimSnapshot`].
#[derive(Debug, Default)]
pub struct Telemetry {
    pub title: String,
    pub atc_model: String,
    pub atc_id: String,

    pub lat: f64,
    pub lon: f64,
    pub altitude_msl_ft: f64,
    pub altitude_agl_ft: f64,

    pub heading_true_deg: f64,
    pub heading_magnetic_deg: f64,
    pub pitch_deg: f64,
    pub bank_deg: f64,
    pub vertical_speed_fpm: f64,

    pub groundspeed_kt: f64,
    pub indicated_airspeed_kt: f64,
    pub true_airspeed_kt: f64,
    pub g_force: f64,

    pub on_ground: bool,
    pub parking_brake: bool,
    pub stall_warning: bool,
    pub overspeed_warning: bool,
    pub gear_position: f64,
    pub flaps_position: f64,
    pub eng1_firing: bool,
    pub eng2_firing: bool,
    pub eng3_firing: bool,
    pub eng4_firing: bool,

    pub fuel_total_lb_ex1: f64,
    pub fuel_total_lb_legacy: f64,
    pub total_weight_lb: f64,
    pub empty_weight_lb: f64,

    pub wind_direction_deg: f64,
    pub wind_speed_kt: f64,
    pub qnh_hpa: f64,
    pub oat_c: f64,
}

impl Telemetry {
    fn from_block(bytes: &[u8]) -> Self {
        // Walk the buffer in TELEMETRY_FIELDS order. If the buffer is
        // shorter than expected (some SimVar got rejected and the
        // tail is missing), every later field stays at its default.
        let mut t = Telemetry::default();
        let mut off = 0usize;

        // Macro-equivalent: pull next field into `dst` if the buffer
        // is long enough. Strings copy the NUL-terminated content.
        macro_rules! pull_f64 {
            ($dst:expr) => {
                if let Some(v) = read_f64(bytes, off) {
                    $dst = v;
                }
                off += 8;
            };
        }
        macro_rules! pull_i32 {
            ($dst:expr) => {
                if let Some(v) = read_i32(bytes, off) {
                    $dst = v != 0;
                }
                off += 4;
            };
        }
        macro_rules! pull_str {
            ($dst:expr) => {
                if let Some(v) = read_str256(bytes, off) {
                    $dst = v;
                }
                off += 256;
            };
        }

        // Same order as TELEMETRY_FIELDS — keep these in lock-step.
        pull_str!(t.title);
        pull_str!(t.atc_model);
        pull_str!(t.atc_id);

        pull_f64!(t.lat);
        pull_f64!(t.lon);
        pull_f64!(t.altitude_msl_ft);
        pull_f64!(t.altitude_agl_ft);

        pull_f64!(t.heading_true_deg);
        pull_f64!(t.heading_magnetic_deg);
        pull_f64!(t.pitch_deg);
        pull_f64!(t.bank_deg);
        pull_f64!(t.vertical_speed_fpm);

        pull_f64!(t.groundspeed_kt);
        pull_f64!(t.indicated_airspeed_kt);
        pull_f64!(t.true_airspeed_kt);
        pull_f64!(t.g_force);

        pull_i32!(t.on_ground);
        pull_i32!(t.parking_brake);
        pull_i32!(t.stall_warning);
        pull_i32!(t.overspeed_warning);
        pull_f64!(t.gear_position);
        pull_f64!(t.flaps_position);
        pull_i32!(t.eng1_firing);
        pull_i32!(t.eng2_firing);
        pull_i32!(t.eng3_firing);
        pull_i32!(t.eng4_firing);

        pull_f64!(t.fuel_total_lb_ex1);
        pull_f64!(t.fuel_total_lb_legacy);
        pull_f64!(t.total_weight_lb);
        pull_f64!(t.empty_weight_lb);

        pull_f64!(t.wind_direction_deg);
        pull_f64!(t.wind_speed_kt);
        pull_f64!(t.qnh_hpa);
        pull_f64!(t.oat_c);

        // Silence the unused-assignment warning the last `pull_*!`
        // emits (the macro always advances `off`, but the very last
        // call doesn't read it again).
        let _ = off;

        t
    }
}

/// Convenience used by the worker: parse + remap to `SimSnapshot`.
pub fn parse(bytes: &[u8], simulator: Simulator) -> SimSnapshot {
    let t = Telemetry::from_block(bytes);
    telemetry_to_snapshot(t, simulator)
}

fn read_f64(bytes: &[u8], off: usize) -> Option<f64> {
    bytes.get(off..off + 8).map(|s| {
        let mut buf = [0u8; 8];
        buf.copy_from_slice(s);
        f64::from_le_bytes(buf)
    })
}

fn read_i32(bytes: &[u8], off: usize) -> Option<i32> {
    bytes.get(off..off + 4).map(|s| {
        let mut buf = [0u8; 4];
        buf.copy_from_slice(s);
        i32::from_le_bytes(buf)
    })
}

fn read_str256(bytes: &[u8], off: usize) -> Option<String> {
    bytes.get(off..off + 256).map(|s| {
        let end = s.iter().position(|b| *b == 0).unwrap_or(s.len());
        String::from_utf8_lossy(&s[..end]).into_owned()
    })
}

fn telemetry_to_snapshot(t: Telemetry, simulator: Simulator) -> SimSnapshot {
    let profile = AircraftProfile::detect(&t.title, &t.atc_model);

    let engines_running = (t.eng1_firing as u8)
        + (t.eng2_firing as u8)
        + (t.eng3_firing as u8)
        + (t.eng4_firing as u8);

    // Fuel: prefer the SU2 EX1 SimVar (works for modern fuel-system
    // aircraft), fall back to the legacy WEIGHT SimVar.
    let fuel_total_lb = if t.fuel_total_lb_ex1 > 0.0 {
        t.fuel_total_lb_ex1
    } else {
        t.fuel_total_lb_legacy
    };
    let fuel_total_kg = (fuel_total_lb * KG_PER_LB) as f32;

    // Gross weight: TOTAL WEIGHT is documented as authoritative.
    let total_weight_kg = if t.total_weight_lb > 0.0 {
        Some((t.total_weight_lb * KG_PER_LB) as f32)
    } else {
        None
    };

    SimSnapshot {
        timestamp: Utc::now(),
        lat: t.lat,
        lon: t.lon,
        altitude_msl_ft: t.altitude_msl_ft,
        altitude_agl_ft: t.altitude_agl_ft,
        heading_deg_true: t.heading_true_deg as f32,
        heading_deg_magnetic: t.heading_magnetic_deg as f32,
        pitch_deg: t.pitch_deg as f32,
        bank_deg: t.bank_deg as f32,
        vertical_speed_fpm: t.vertical_speed_fpm as f32,
        groundspeed_kt: t.groundspeed_kt as f32,
        indicated_airspeed_kt: t.indicated_airspeed_kt as f32,
        true_airspeed_kt: t.true_airspeed_kt as f32,
        g_force: t.g_force as f32,
        on_ground: t.on_ground,
        parking_brake: t.parking_brake,
        stall_warning: t.stall_warning,
        overspeed_warning: t.overspeed_warning,
        paused: false,
        slew_mode: false,
        simulation_rate: 1.0,
        gear_position: t.gear_position as f32,
        flaps_position: t.flaps_position as f32,
        engines_running,
        fuel_total_kg,
        fuel_used_kg: 0.0,
        zfw_kg: None,
        payload_kg: None,
        total_weight_kg,
        // Touchdown sample: not yet wired in raw mode; stays None
        // until we add a second data definition for them. The legacy
        // adapter also kept these None.
        touchdown_vs_fpm: None,
        touchdown_pitch_deg: None,
        touchdown_bank_deg: None,
        touchdown_heading_mag_deg: None,
        touchdown_lat: None,
        touchdown_lon: None,
        wind_direction_deg: Some(t.wind_direction_deg as f32),
        wind_speed_kt: Some(t.wind_speed_kt as f32),
        qnh_hpa: Some(t.qnh_hpa as f32),
        outside_air_temp_c: Some(t.oat_c as f32),
        aircraft_title: Some(t.title).filter(|s| !s.is_empty()),
        aircraft_icao: Some(t.atc_model).filter(|s| !s.is_empty()),
        aircraft_registration: Some(t.atc_id).filter(|s| !s.is_empty()),
        simulator,
        sim_version: None,
        // Avionics / lights / AP: not yet ported to raw FFI. They
        // come back in the next iteration once the foundation is
        // proven. Until then the rest of the app sees None and skips
        // those fields cleanly.
        transponder_code: None,
        com1_mhz: None,
        com2_mhz: None,
        nav1_mhz: None,
        nav2_mhz: None,
        light_landing: None,
        light_beacon: None,
        light_strobe: None,
        light_taxi: None,
        light_nav: None,
        light_logo: None,
        autopilot_master: None,
        autopilot_heading: None,
        autopilot_altitude: None,
        autopilot_nav: None,
        autopilot_approach: None,
        fuel_flow_kg_per_h: None,
        parking_name: None,
        parking_number: None,
        selected_runway: None,
        aircraft_profile: profile,
    }
}
