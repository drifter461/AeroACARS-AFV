//! CloudeAcars — Tauri application root.
//!
//! Holds the active `api_client::Client` in shared state, exposes auth commands
//! to the UI (login, logout, session restore), and persists the site URL to a
//! per-user config dir. The API key itself is stored via `secrets` (OS keyring),
//! never on disk in plaintext.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use api_client::{
    Airport, ApiError, Bid, Client, Connection, FareEntry, FileBody, PositionEntry, PrefileBody,
    Profile, UpdateBody,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sim_core::{FlightPhase, SimKind, SimSnapshot};
use tauri::{AppHandle, Manager};
use tracing_subscriber::EnvFilter;

#[cfg(target_os = "windows")]
use sim_msfs::MsfsAdapter;

const KEYRING_ACCOUNT: &str = "primary";
const SITE_CONFIG_FILE: &str = "site.json";
const SIM_CONFIG_FILE: &str = "sim.json";
/// File holding the current in-progress flight, written on flight_start and
/// removed on flight_end / flight_cancel. Lets us resume after a client crash.
const ACTIVE_FLIGHT_FILE: &str = "active_flight.json";

/// Anything older than this is considered stale and discarded on resume.
const RESUME_MAX_AGE_HOURS: i64 = 12;

/// How often the background task posts the latest position to phpVMS while a
/// flight is active. Spec §10 talks about "configurable intervals"; for now we
/// hard-code a sane default and make it tunable later.
const POSITION_INTERVAL_SECS: u64 = 10;

/// Minimum great-circle distance between two consecutive samples before we
/// add it to the running total. Filters out GPS jitter while parked.
const DISTANCE_EPSILON_M: f64 = 5.0;

/// How close (in nautical miles) the aircraft must be to the departure airport
/// to start the flight. Generous enough to cover taxi positions and remote
/// stands; tight enough to reject "I'm at EDDF instead of EDDP".
const MAX_START_DISTANCE_NM: f64 = 5.0;

/// MSFS often returns SimVar values as localization keys, not plain text.
/// The ATC MODEL var is one of them — e.g. `TT:ATCCOM.AC_MODEL_A320.0.text`
/// or `ATCCOM.AC_MODEL A320.0.text`. Pull out the readable code, or return
/// `None` if the input is an unresolved key we can't decode.
fn clean_atc_model(raw: &str) -> Option<String> {
    let s = raw.trim();
    if s.is_empty() {
        return None;
    }
    if let Some(start) = s.find("AC_MODEL") {
        let after = &s[start + "AC_MODEL".len()..];
        let after = after.trim_start_matches(|c: char| c == '_' || c == ' ');
        if let Some(end) = after.find('.') {
            let model = &after[..end];
            if !model.is_empty() {
                return Some(model.to_uppercase());
            }
        }
    }
    let upper = s.to_uppercase();
    if upper.starts_with("TT:") || upper.contains("ATCCOM.") || upper.ends_with(".TEXT") {
        return None;
    }
    Some(upper)
}

/// Loose check: does the aircraft title from MSFS appear to mention the given
/// ICAO code? Used as a permissive backup when ATC MODEL parses to one code
/// but the title says something completely different.
fn title_mentions_icao(title: &str, icao: &str) -> bool {
    let title_upper = title.to_uppercase();
    let icao_upper = icao.to_uppercase();
    title_upper.contains(&icao_upper)
}

/// Shared application state — wraps the currently-authenticated client (if any)
/// and (on Windows) the MSFS adapter.
#[derive(Default)]
struct AppState {
    client: Mutex<Option<Client>>,
    #[cfg(target_os = "windows")]
    msfs: Mutex<MsfsAdapter>,
    active_flight: Mutex<Option<Arc<ActiveFlight>>>,
    /// In-process airport-coords cache. Keyed by ICAO uppercase. Populated on
    /// first lookup so we don't re-fetch on every snapshot tick.
    airports: Mutex<HashMap<String, Airport>>,
}

/// In-memory record of an in-progress flight. Held inside an `Arc` so the
/// background streaming task can hold a reference without going through the
/// AppState mutex.
struct ActiveFlight {
    pirep_id: String,
    bid_id: i64,
    started_at: DateTime<Utc>,
    flight_number: String,
    dpt_airport: String,
    arr_airport: String,
    /// Final loads (per fare-class id) captured at flight start so we can
    /// include them in the filed PIREP — even if the bid is gone by then.
    fares: Vec<(i64, i32)>,
    /// Mutable running stats updated by the streamer task.
    stats: Mutex<FlightStats>,
    stop: AtomicBool,
}

/// On-disk representation of an active flight, used for resume after a client
/// crash. Not the same as `ActiveFlight` because we only persist serializable,
/// non-Mutex fields.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct PersistedFlight {
    pirep_id: String,
    bid_id: i64,
    started_at: DateTime<Utc>,
    flight_number: String,
    dpt_airport: String,
    arr_airport: String,
    fares: Vec<(i64, i32)>,
}

#[derive(Default)]
struct FlightStats {
    // Position tracking.
    last_lat: Option<f64>,
    last_lon: Option<f64>,
    distance_nm: f64,
    position_count: u32,

    // ---- Phase-FSM state ----
    /// Current flight phase. Starts at Boarding when flight_start fires.
    phase: FlightPhase,
    /// Recent transitions for the flight log.
    transitions: Vec<(DateTime<Utc>, FlightPhase)>,
    /// Snapshot of the previous tick — used to detect on_ground / parking
    /// brake transitions cleanly.
    was_on_ground: Option<bool>,
    was_parking_brake: Option<bool>,

    // ---- Block / takeoff / landing timestamps (real-time UTC) ----
    block_off_at: Option<DateTime<Utc>>,
    takeoff_at: Option<DateTime<Utc>>,
    landing_at: Option<DateTime<Utc>>,
    block_on_at: Option<DateTime<Utc>>,

    // ---- Capture at takeoff ----
    takeoff_weight_kg: Option<f64>,
    takeoff_fuel_kg: Option<f32>,

    // ---- Capture at touchdown ----
    landing_rate_fpm: Option<f32>,
    landing_g_force: Option<f32>,
    landing_pitch_deg: Option<f32>,
    landing_speed_kt: Option<f32>,
    landing_weight_kg: Option<f64>,
    landing_heading_deg: Option<f32>,
    landing_fuel_kg: Option<f32>,

    // ---- Fuel tracking ----
    block_fuel_kg: Option<f32>,
    last_fuel_kg: Option<f32>,
}

impl FlightStats {
    fn new() -> Self {
        Self {
            phase: FlightPhase::Boarding,
            ..Self::default()
        }
    }
}

#[derive(Serialize)]
pub struct ActiveFlightInfo {
    pirep_id: String,
    bid_id: i64,
    started_at: String,
    flight_number: String,
    dpt_airport: String,
    arr_airport: String,
    distance_nm: f64,
    position_count: u32,
    /// snake_case name of the current `FlightPhase` (e.g. "boarding", "climb").
    phase: String,
    /// ISO-8601 timestamps captured at major flight events. Each is `null`
    /// until the corresponding transition fires.
    block_off_at: Option<String>,
    takeoff_at: Option<String>,
    landing_at: Option<String>,
    block_on_at: Option<String>,
    landing_rate_fpm: Option<f32>,
    landing_g_force: Option<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SiteConfig {
    url: String,
}

#[derive(Serialize)]
pub struct LoginResult {
    profile: Profile,
    base_url: String,
}

/// Errors returned to the UI in a serializable shape.
/// `code` is a stable, machine-readable identifier the frontend uses for i18n.
#[derive(Debug, Serialize)]
pub struct UiError {
    code: String,
    message: String,
}

impl UiError {
    fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }
}

impl From<ApiError> for UiError {
    fn from(err: ApiError) -> Self {
        Self {
            code: err.code().to_string(),
            message: err.to_string(),
        }
    }
}

// ---- Site config persistence ----

fn site_config_path(app: &AppHandle) -> Result<PathBuf, UiError> {
    app.path()
        .app_config_dir()
        .map(|dir| dir.join(SITE_CONFIG_FILE))
        .map_err(|e| UiError::new("config_path", e.to_string()))
}

fn read_site_config(app: &AppHandle) -> Result<Option<SiteConfig>, UiError> {
    let path = site_config_path(app)?;
    if !path.exists() {
        return Ok(None);
    }
    let bytes =
        std::fs::read(&path).map_err(|e| UiError::new("config_read", e.to_string()))?;
    let cfg: SiteConfig = serde_json::from_slice(&bytes)
        .map_err(|e| UiError::new("config_parse", e.to_string()))?;
    Ok(Some(cfg))
}

fn write_site_config(app: &AppHandle, cfg: &SiteConfig) -> Result<(), UiError> {
    let path = site_config_path(app)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| UiError::new("config_write", e.to_string()))?;
    }
    let json = serde_json::to_vec_pretty(cfg)
        .map_err(|e| UiError::new("config_serialize", e.to_string()))?;
    std::fs::write(&path, json).map_err(|e| UiError::new("config_write", e.to_string()))
}

fn clear_site_config(app: &AppHandle) -> Result<(), UiError> {
    let path = site_config_path(app)?;
    if path.exists() {
        std::fs::remove_file(&path)
            .map_err(|e| UiError::new("config_remove", e.to_string()))?;
    }
    Ok(())
}

// ---- Tauri commands ----

#[derive(Serialize)]
pub struct AppInfo {
    pub name: &'static str,
    pub version: &'static str,
    pub commit: Option<&'static str>,
}

#[tauri::command]
fn app_info() -> AppInfo {
    AppInfo {
        name: "CloudeAcars",
        version: env!("CARGO_PKG_VERSION"),
        commit: option_env!("CLOUDEACARS_GIT_SHA"),
    }
}

/// Authenticate against a phpVMS site. On success: stores key in OS keyring,
/// writes URL to site config, and caches the live `Client` in `AppState`.
#[tauri::command]
async fn phpvms_login(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    url: String,
    api_key: String,
) -> Result<LoginResult, UiError> {
    let conn = Connection::new(&url, api_key.trim())?;
    let client = Client::new(conn)?;
    let profile = client.get_profile().await?;

    secrets::store_api_key(KEYRING_ACCOUNT, api_key.trim())
        .map_err(|e| UiError::new("keyring", e.to_string()))?;
    write_site_config(&app, &SiteConfig { url: url.clone() })?;

    let base_url = client.connection().base_url().to_string();
    *state.client.lock().expect("client mutex") = Some(client.clone());

    // Auto-start the simulator adapter using the persisted selection.
    let saved_kind = read_sim_config(&app).kind;
    apply_sim_kind(&state, saved_kind);

    // Try to resume an in-progress flight (e.g. after a client crash).
    try_resume_flight(&app, &state, &client);

    tracing::info!(pilot = profile.name.as_str(), ?saved_kind, "logged in");
    Ok(LoginResult { profile, base_url })
}

/// Forget the current session. Removes the keyring entry and site config,
/// clears the in-memory client.
#[tauri::command]
async fn phpvms_logout(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<(), UiError> {
    *state.client.lock().expect("client mutex") = None;
    secrets::delete_api_key(KEYRING_ACCOUNT)
        .map_err(|e| UiError::new("keyring", e.to_string()))?;
    clear_site_config(&app)?;
    tracing::info!("logged out");
    Ok(())
}

/// On app launch: try to restore the previous session from disk + keyring.
/// Returns `None` if no session is stored or stored key is now invalid.
#[tauri::command]
async fn phpvms_load_session(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<Option<LoginResult>, UiError> {
    let Some(cfg) = read_site_config(&app)? else {
        return Ok(None);
    };
    let Some(api_key) = secrets::load_api_key(KEYRING_ACCOUNT)
        .map_err(|e| UiError::new("keyring", e.to_string()))?
    else {
        return Ok(None);
    };

    let conn = Connection::new(&cfg.url, &api_key)?;
    let client = Client::new(conn)?;
    match client.get_profile().await {
        Ok(profile) => {
            let base_url = client.connection().base_url().to_string();
            *state.client.lock().expect("client mutex") = Some(client.clone());
            // Auto-start the simulator adapter when we restore an existing session.
            let saved_kind = read_sim_config(&app).kind;
            apply_sim_kind(&state, saved_kind);
            try_resume_flight(&app, &state, &client);
            tracing::info!(?saved_kind, "session restored");
            Ok(Some(LoginResult { profile, base_url }))
        }
        // Stored key was rejected — drop it so the next login goes via the form.
        Err(ApiError::Unauthenticated) => {
            let _ = secrets::delete_api_key(KEYRING_ACCOUNT);
            let _ = clear_site_config(&app);
            Ok(None)
        }
        Err(other) => Err(other.into()),
    }
}

/// Pull the active client out of state, or fail with `not_logged_in`.
fn current_client(state: &tauri::State<'_, AppState>) -> Result<Client, UiError> {
    let guard = state.client.lock().expect("client mutex");
    guard
        .as_ref()
        .cloned()
        .ok_or_else(|| UiError::new("not_logged_in", "no active session"))
}

/// `GET /api/user/bids` — the pilot's open bids.
#[tauri::command]
async fn phpvms_get_bids(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<Bid>, UiError> {
    let client = current_client(&state)?;
    Ok(client.get_bids().await?)
}

// ---- Active-flight persistence (for resume after crash/restart) ----

fn active_flight_path(app: &AppHandle) -> Result<PathBuf, UiError> {
    app.path()
        .app_config_dir()
        .map(|dir| dir.join(ACTIVE_FLIGHT_FILE))
        .map_err(|e| UiError::new("config_path", e.to_string()))
}

fn write_persisted_flight(app: &AppHandle, flight: &PersistedFlight) -> Result<(), UiError> {
    let path = active_flight_path(app)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| UiError::new("config_write", e.to_string()))?;
    }
    let bytes = serde_json::to_vec_pretty(flight)
        .map_err(|e| UiError::new("config_serialize", e.to_string()))?;
    std::fs::write(&path, bytes).map_err(|e| UiError::new("config_write", e.to_string()))
}

fn read_persisted_flight(app: &AppHandle) -> Option<PersistedFlight> {
    let path = active_flight_path(app).ok()?;
    if !path.exists() {
        return None;
    }
    let bytes = std::fs::read(&path).ok()?;
    serde_json::from_slice::<PersistedFlight>(&bytes).ok()
}

fn clear_persisted_flight(app: &AppHandle) {
    let Ok(path) = active_flight_path(app) else { return };
    if path.exists() {
        let _ = std::fs::remove_file(&path);
    }
}

fn save_active_flight(app: &AppHandle, flight: &ActiveFlight) {
    let persisted = PersistedFlight {
        pirep_id: flight.pirep_id.clone(),
        bid_id: flight.bid_id,
        started_at: flight.started_at,
        flight_number: flight.flight_number.clone(),
        dpt_airport: flight.dpt_airport.clone(),
        arr_airport: flight.arr_airport.clone(),
        fares: flight.fares.clone(),
    };
    if let Err(e) = write_persisted_flight(app, &persisted) {
        tracing::warn!(error = ?e, "could not persist active flight");
    }
}

// ---- Airport cache ----

#[derive(Serialize)]
pub struct AirportInfo {
    icao: String,
    name: Option<String>,
    lat: Option<f64>,
    lon: Option<f64>,
}

/// Fetch an airport by ICAO, caching the result so we don't re-hit the network
/// on each sim snapshot.
#[tauri::command]
async fn airport_get(
    state: tauri::State<'_, AppState>,
    icao: String,
) -> Result<AirportInfo, UiError> {
    let key = icao.trim().to_uppercase();
    // Block-scope the lock so the MutexGuard is dropped before any `await`,
    // keeping the future `Send`.
    let cached: Option<Airport> = {
        let guard = state.airports.lock().expect("airports lock");
        guard.get(&key).cloned()
    };
    if let Some(c) = cached {
        return Ok(AirportInfo {
            icao: key,
            name: c.name,
            lat: c.lat,
            lon: c.lon,
        });
    }
    let client = current_client(&state)?;
    let airport = client.get_airport(&key).await?;
    let info = AirportInfo {
        icao: key.clone(),
        name: airport.name.clone(),
        lat: airport.lat,
        lon: airport.lon,
    };
    {
        let mut guard = state.airports.lock().expect("airports lock");
        guard.insert(key, airport);
    }
    Ok(info)
}

// ---- Flight workflow ----

fn flight_info(flight: &ActiveFlight) -> ActiveFlightInfo {
    let stats = flight.stats.lock().expect("flight stats");
    ActiveFlightInfo {
        pirep_id: flight.pirep_id.clone(),
        bid_id: flight.bid_id,
        started_at: flight.started_at.to_rfc3339(),
        flight_number: flight.flight_number.clone(),
        dpt_airport: flight.dpt_airport.clone(),
        arr_airport: flight.arr_airport.clone(),
        distance_nm: stats.distance_nm,
        position_count: stats.position_count,
        phase: phase_to_snake(stats.phase).to_string(),
        block_off_at: stats.block_off_at.map(|t| t.to_rfc3339()),
        takeoff_at: stats.takeoff_at.map(|t| t.to_rfc3339()),
        landing_at: stats.landing_at.map(|t| t.to_rfc3339()),
        block_on_at: stats.block_on_at.map(|t| t.to_rfc3339()),
        landing_rate_fpm: stats.landing_rate_fpm,
        landing_g_force: stats.landing_g_force,
    }
}

fn phase_to_snake(phase: FlightPhase) -> &'static str {
    match phase {
        FlightPhase::Preflight => "preflight",
        FlightPhase::Boarding => "boarding",
        FlightPhase::Pushback => "pushback",
        FlightPhase::TaxiOut => "taxi_out",
        FlightPhase::TakeoffRoll => "takeoff_roll",
        FlightPhase::Takeoff => "takeoff",
        FlightPhase::Climb => "climb",
        FlightPhase::Cruise => "cruise",
        FlightPhase::Descent => "descent",
        FlightPhase::Approach => "approach",
        FlightPhase::Final => "final",
        FlightPhase::Landing => "landing",
        FlightPhase::TaxiIn => "taxi_in",
        FlightPhase::BlocksOn => "blocks_on",
        FlightPhase::Arrived => "arrived",
        FlightPhase::PirepSubmitted => "pirep_submitted",
    }
}

#[tauri::command]
fn flight_status(state: tauri::State<'_, AppState>) -> Option<ActiveFlightInfo> {
    let guard = state.active_flight.lock().expect("active_flight lock");
    guard.as_ref().map(|f| flight_info(f.as_ref()))
}

/// Start tracking a flight: prefile a PIREP and begin position streaming.
#[tauri::command]
async fn flight_start(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    bid_id: i64,
) -> Result<ActiveFlightInfo, UiError> {
    {
        let guard = state.active_flight.lock().expect("active_flight lock");
        if guard.is_some() {
            return Err(UiError::new(
                "flight_already_active",
                "another flight is already active",
            ));
        }
    }

    let client = current_client(&state)?;
    let bids = client.get_bids().await?;
    let bid = bids
        .into_iter()
        .find(|b| b.id == bid_id)
        .ok_or_else(|| UiError::new("bid_not_found", "bid not found in current bids"))?;

    // ---- Pre-flight gating: must be on the ground at the departure airport ----
    let snapshot = current_snapshot(&app).ok_or_else(|| {
        UiError::new("no_sim_snapshot", "no sim snapshot yet — is the simulator connected?")
    })?;
    if !snapshot.on_ground {
        return Err(UiError::new(
            "not_on_ground",
            "you must be on the ground to start a flight",
        ));
    }

    // Cached or live fetch of the departure airport. The lock is taken in a
    // narrow scope each time so the MutexGuard never crosses an `await`.
    let dpt_icao = bid.flight.dpt_airport_id.trim().to_uppercase();
    let cached_dpt: Option<Airport> = {
        let guard = state.airports.lock().expect("airports lock");
        guard.get(&dpt_icao).cloned()
    };
    let dpt_airport = match cached_dpt {
        Some(a) => a,
        None => {
            let fetched = client.get_airport(&dpt_icao).await?;
            let mut guard = state.airports.lock().expect("airports lock");
            guard.insert(dpt_icao.clone(), fetched.clone());
            fetched
        }
    };
    if let (Some(lat), Some(lon)) = (dpt_airport.lat, dpt_airport.lon) {
        let distance_nm =
            ::geo::distance_m(snapshot.lat, snapshot.lon, lat, lon) / 1852.0;
        if distance_nm > MAX_START_DISTANCE_NM {
            return Err(UiError::new(
                "not_at_departure",
                format!(
                    "you are {:.1} nm from {} — start the flight at the departure airport",
                    distance_nm, dpt_icao
                ),
            ));
        }
        tracing::info!(
            dpt = %dpt_icao,
            distance_nm,
            "preflight gate passed"
        );
    } else {
        tracing::warn!(
            dpt = %dpt_icao,
            "no coordinates for departure airport — skipping distance check"
        );
    }

    let airline_id = bid.flight.airline.as_ref().map(|a| a.id).ok_or_else(|| {
        UiError::new("missing_airline", "bid has no airline relation")
    })?;
    let aircraft_id = bid
        .flight
        .simbrief
        .as_ref()
        .map(|sb| sb.aircraft_id)
        .flatten()
        .ok_or_else(|| {
            UiError::new(
                "missing_aircraft",
                "no aircraft on this bid — please prepare a SimBrief OFP first",
            )
        })?;

    // ---- Aircraft-mismatch gate (spec §7) ----
    // Compare the aircraft type the bid expects (from get_aircraft) to what's
    // loaded in the simulator (parsed ATC MODEL or, as a backup, the TITLE).
    // Permissive: only block when both sides resolve to clearly different codes.
    let expected_aircraft = client.get_aircraft(aircraft_id).await?;
    let expected_icao = expected_aircraft
        .icao
        .as_ref()
        .map(|s| s.trim().to_uppercase())
        .filter(|s| !s.is_empty());
    let sim_icao = snapshot
        .aircraft_icao
        .as_deref()
        .and_then(clean_atc_model);
    let sim_title = snapshot
        .aircraft_title
        .as_deref()
        .unwrap_or("")
        .to_string();
    if let (Some(expected), Some(actual)) = (expected_icao.as_ref(), sim_icao.as_ref()) {
        let title_supports_expected = title_mentions_icao(&sim_title, expected);
        if expected != actual && !title_supports_expected {
            let registration = expected_aircraft
                .registration
                .as_deref()
                .unwrap_or("?");
            tracing::warn!(
                expected = %expected,
                actual = %actual,
                title = %sim_title,
                registration = %registration,
                "aircraft type mismatch — blocking flight start"
            );
            return Err(UiError::new(
                "aircraft_mismatch",
                format!(
                    "Aircraft mismatch: bid wants {expected} ({registration}), sim has {actual} (title \"{sim_title}\"). Load the correct aircraft type in the sim or pick a matching bid.",
                ),
            ));
        }
    }

    let body = PrefileBody {
        airline_id,
        aircraft_id: aircraft_id.to_string(),
        flight_number: bid.flight.flight_number.clone(),
        dpt_airport_id: bid.flight.dpt_airport_id.clone(),
        arr_airport_id: bid.flight.arr_airport_id.clone(),
        alt_airport_id: bid.flight.alt_airport_id.clone(),
        flight_type: bid.flight.flight_type.clone(),
        route_code: bid.flight.route_code.clone(),
        route_leg: bid.flight.route_leg.clone(),
        level: bid.flight.level.filter(|&l| l > 0),
        planned_distance: bid.flight.distance.as_ref().and_then(|d| d.nmi),
        planned_flight_time: bid.flight.flight_time,
        route: bid.flight.route.clone().filter(|s| !s.is_empty()),
        source_name: format!("CloudeAcars/{}", env!("CARGO_PKG_VERSION")),
        notes: None,
    };

    // Before trying a fresh prefile, see if the user already has an in-progress
    // PIREP for this flight. This handles the "client crashed / persistence
    // file gone, but phpVMS still has the active PIREP" case — we adopt the
    // existing PIREP instead of trying to create a new one (which would fail
    // with aircraft-not-available because the aircraft is already "in use" by
    // the orphaned PIREP).
    let existing = match client.get_user_pireps().await {
        Ok(list) => list,
        Err(e) => {
            tracing::warn!(error = %e, "could not list user PIREPs to check for resume");
            Vec::new()
        }
    };
    let adoptable = existing.into_iter().find(|p| {
        // phpVMS PirepState IN_PROGRESS = 0.
        p.state == Some(0)
            && p.flight_number.as_deref() == Some(body.flight_number.as_str())
            && (p.airline_id.is_none() || p.airline_id == Some(airline_id))
    });
    if let Some(p) = &adoptable {
        tracing::info!(pirep_id = %p.id, "adopting existing in-progress PIREP");
    }

    tracing::info!(
        airline_id,
        aircraft_id,
        flight_number = body.flight_number.as_str(),
        adopting = adoptable.is_some(),
        "prefiling PIREP"
    );
    let pirep = if let Some(adopt) = adoptable {
        api_client::PirepCreated { id: adopt.id }
    } else {
    match client.prefile_pirep(&body).await {
        Ok(p) => p,
        Err(ApiError::Server { status: 400, body: err_body })
            if err_body.contains("aircraft-not-available") =>
        {
            // Diagnose: fetch aircraft details to tell the user *why* it's
            // unavailable (wrong airport, "in use" by an orphan PIREP, etc.).
            let detail = match client.get_aircraft(aircraft_id).await {
                Ok(a) => {
                    let reg = a
                        .registration
                        .as_deref()
                        .or(a.name.as_deref())
                        .unwrap_or("?");
                    let where_ = a.airport_id.as_deref().unwrap_or("?");
                    let state = match a.state {
                        Some(0) => "parked",
                        Some(1) => "in use",
                        Some(2) => "in flight",
                        _ => "unknown",
                    };
                    format!(
                        "{reg} (id {}): currently at {where_}, state '{state}'. Wanted at {dpt_icao}.",
                        a.id
                    )
                }
                Err(e) => format!(
                    "could not fetch aircraft {} details: {e}",
                    aircraft_id
                ),
            };
            tracing::warn!(aircraft_id, %detail, "aircraft not available");
            return Err(UiError::new(
                "aircraft_not_available",
                format!("Aircraft not available — {detail}"),
            ));
        }
        Err(ApiError::Server { status, body: err_body }) => {
            // Try to extract a human-readable message from a phpVMS JSON error body.
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&err_body) {
                if let Some(title) = json.get("title").and_then(|v| v.as_str()) {
                    return Err(UiError::new(
                        "phpvms_error",
                        format!("phpVMS rejected the flight (HTTP {status}): {title}"),
                    ));
                }
            }
            return Err(UiError::new(
                "phpvms_error",
                format!("phpVMS rejected the flight (HTTP {status})"),
            ));
        }
        Err(e) => return Err(e.into()),
        }
    };

    // Advance the PIREP status to BOARDING and ensure state is IN_PROGRESS so
    // it appears in phpVMS's "Aktive Flüge" view.
    //
    // phpVMS 7 PirepState values: REJECTED = -1, IN_PROGRESS = 0, PENDING = 1,
    // ACCEPTED = 2. We send 0 explicitly so this also recovers any PIREP that
    // accidentally got pushed to PENDING earlier (e.g. by a buggier client).
    let update_body = UpdateBody {
        state: Some(0),
        status: Some("BST".to_string()),
        notes: None,
    };
    if let Err(e) = client.update_pirep(&pirep.id, &update_body).await {
        tracing::warn!(
            pirep_id = %pirep.id,
            error = %e,
            "could not advance PIREP status to BOARDING (flight will still be tracked)"
        );
    } else {
        tracing::info!(pirep_id = %pirep.id, "PIREP status set to BOARDING");
    }

    // Capture fares from the SimBrief OFP so we can file accurate loads
    // even if the bid is gone by the time we end the flight.
    let fares: Vec<(i64, i32)> = bid
        .flight
        .simbrief
        .as_ref()
        .and_then(|sb| sb.subfleet.as_ref())
        .map(|sf| {
            sf.fares
                .iter()
                .filter_map(|f| f.count.map(|c| (f.id, c)))
                .collect()
        })
        .unwrap_or_default();

    let flight = Arc::new(ActiveFlight {
        pirep_id: pirep.id.clone(),
        bid_id,
        started_at: Utc::now(),
        flight_number: bid.flight.flight_number.clone(),
        dpt_airport: bid.flight.dpt_airport_id.clone(),
        arr_airport: bid.flight.arr_airport_id.clone(),
        fares,
        stats: Mutex::new(FlightStats::new()),
        stop: AtomicBool::new(false),
    });

    save_active_flight(&app, &flight);

    {
        let mut guard = state.active_flight.lock().expect("active_flight lock");
        *guard = Some(Arc::clone(&flight));
    }

    spawn_position_streamer(app.clone(), Arc::clone(&flight), client);

    let info = flight_info(flight.as_ref());
    tracing::info!(pirep_id = %flight.pirep_id, "flight started");
    Ok(info)
}

/// File the active PIREP with computed final stats.
#[tauri::command]
async fn flight_end(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<(), UiError> {
    let flight = {
        let mut guard = state.active_flight.lock().expect("active_flight lock");
        guard
            .take()
            .ok_or_else(|| UiError::new("no_active_flight", "no flight is active"))?
    };
    flight.stop.store(true, Ordering::Relaxed);

    let client = current_client(&state)?;

    // Snapshot all stats inside a single short-lived guard to avoid holding
    // the Mutex across an `await`.
    let body = {
        let stats = flight.stats.lock().expect("flight stats");
        let elapsed_minutes = (Utc::now() - flight.started_at).num_minutes() as i32;

        let fares = if flight.fares.is_empty() {
            None
        } else {
            Some(
                flight
                    .fares
                    .iter()
                    .map(|(id, count)| FareEntry {
                        id: *id,
                        count: *count,
                    })
                    .collect(),
            )
        };

        let fuel_used = match (stats.block_fuel_kg, stats.last_fuel_kg) {
            (Some(b), Some(c)) if b > c => Some((b - c) as f64),
            _ => None,
        };
        let distance_nm = stats.distance_nm;
        let fields = build_pirep_fields(&flight, &stats);
        let notes = build_pirep_notes(&flight, &stats);

        FileBody {
            flight_time: Some(elapsed_minutes.max(0)),
            fuel_used,
            distance: Some(distance_nm),
            source_name: Some(format!("CloudeAcars/{}", env!("CARGO_PKG_VERSION"))),
            notes: Some(notes),
            fares,
            fields: Some(fields),
        }
    };
    tracing::info!(
        pirep_id = %flight.pirep_id,
        flight_time = body.flight_time.unwrap_or(0),
        distance = body.distance.unwrap_or(0.0),
        fuel_used = body.fuel_used.unwrap_or(0.0),
        fare_classes = flight.fares.len(),
        custom_fields = body.fields.as_ref().map(|f| f.len()).unwrap_or(0),
        "filing PIREP"
    );
    let result = client.file_pirep(&flight.pirep_id, &body).await;
    // Always clear local persistence — if filing failed, the user can either
    // retry via support tooling or just start a fresh flight.
    clear_persisted_flight(&app);
    result?;
    tracing::info!(pirep_id = %flight.pirep_id, "PIREP filed");
    Ok(())
}

/// Cancel the active PIREP without filing it.
#[tauri::command]
async fn flight_cancel(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<(), UiError> {
    let flight = {
        let mut guard = state.active_flight.lock().expect("active_flight lock");
        guard
            .take()
            .ok_or_else(|| UiError::new("no_active_flight", "no flight is active"))?
    };
    flight.stop.store(true, Ordering::Relaxed);
    let client = current_client(&state)?;
    let result = client.cancel_pirep(&flight.pirep_id).await;
    // Clear local persistence regardless — the user wants this gone.
    clear_persisted_flight(&app);
    result?;
    tracing::info!(pirep_id = %flight.pirep_id, "PIREP cancelled");
    Ok(())
}

/// Drop local active-flight state without contacting phpVMS. Useful when the
/// stored PIREP is orphaned/dead on the server side and the user wants a
/// clean slate.
#[tauri::command]
async fn flight_forget(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<(), UiError> {
    if let Some(flight) = state
        .active_flight
        .lock()
        .expect("active_flight lock")
        .take()
    {
        flight.stop.store(true, Ordering::Relaxed);
        tracing::info!(pirep_id = %flight.pirep_id, "active flight forgotten (no phpVMS call)");
    }
    clear_persisted_flight(&app);
    Ok(())
}

/// Spawn the background task that pushes the latest sim snapshot to phpVMS at
/// `POSITION_INTERVAL_SECS`. Stops when `flight.stop` is set or the active
/// flight is replaced.
///
/// We use `tauri::async_runtime::spawn` rather than bare `tokio::spawn` so the
/// task always lands on Tauri's runtime, regardless of feature flags.
fn spawn_position_streamer(app: AppHandle, flight: Arc<ActiveFlight>, client: Client) {
    tauri::async_runtime::spawn(async move {
        tracing::info!(pirep_id = %flight.pirep_id, "position streamer started");
        let mut interval =
            tokio::time::interval(Duration::from_secs(POSITION_INTERVAL_SECS));
        // Skip the immediate first tick so we don't post before we have a snapshot.
        interval.tick().await;
        loop {
            interval.tick().await;
            if flight.stop.load(Ordering::Relaxed) {
                break;
            }

            let snapshot = current_snapshot(&app);
            let Some(snap) = snapshot else {
                tracing::warn!(
                    pirep_id = %flight.pirep_id,
                    "no sim snapshot yet — skipping position post"
                );
                continue;
            };

            // Update running stats AND step the flight-phase FSM.
            let phase_change = step_flight(&flight, &snap);
            let position = snapshot_to_position(&snap);

            match client
                .post_positions(&flight.pirep_id, &[position])
                .await
            {
                Ok(()) => {
                    tracing::info!(
                        pirep_id = %flight.pirep_id,
                        lat = snap.lat,
                        lon = snap.lon,
                        alt_msl_ft = snap.altitude_msl_ft,
                        gs_kt = snap.groundspeed_kt,
                        "position posted"
                    );
                }
                Err(e) => {
                    tracing::warn!(
                        pirep_id = %flight.pirep_id,
                        error = %e,
                        "position post failed; will retry on next tick"
                    );
                }
            }

            // On phase change, push the new status to phpVMS so the
            // live-map and PIREP detail page reflect the current phase.
            if let Some(new_phase) = phase_change {
                if let Some(status) = phase_to_status(new_phase) {
                    tracing::info!(
                        pirep_id = %flight.pirep_id,
                        ?new_phase,
                        status,
                        "flight phase transition"
                    );
                    let body = UpdateBody {
                        state: None,
                        status: Some(status.to_string()),
                        notes: None,
                    };
                    if let Err(e) = client.update_pirep(&flight.pirep_id, &body).await {
                        tracing::warn!(
                            pirep_id = %flight.pirep_id,
                            ?new_phase,
                            error = %e,
                            "could not push phase status update"
                        );
                    }
                }
            }
        }
        tracing::info!(pirep_id = %flight.pirep_id, "position streamer stopped");
    });
}

#[cfg(target_os = "windows")]
fn current_snapshot(app: &AppHandle) -> Option<SimSnapshot> {
    let state = app.state::<AppState>();
    let adapter = state.msfs.lock().expect("msfs lock");
    adapter.snapshot()
}

#[cfg(not(target_os = "windows"))]
fn current_snapshot(_app: &AppHandle) -> Option<SimSnapshot> {
    None
}

/// Update running stats AND step the flight-phase FSM. Returns the new phase
/// when a transition fires, otherwise `None`.
fn step_flight(flight: &ActiveFlight, snap: &SimSnapshot) -> Option<FlightPhase> {
    let mut stats = flight.stats.lock().expect("flight stats");

    // Distance accounting.
    if let (Some(prev_lat), Some(prev_lon)) = (stats.last_lat, stats.last_lon) {
        let d_m = ::geo::distance_m(prev_lat, prev_lon, snap.lat, snap.lon);
        if d_m > DISTANCE_EPSILON_M {
            stats.distance_nm += d_m / 1852.0;
        }
    }
    stats.last_lat = Some(snap.lat);
    stats.last_lon = Some(snap.lon);
    stats.position_count = stats.position_count.saturating_add(1);
    stats.last_fuel_kg = Some(snap.fuel_total_kg);

    // Capture block fuel on the very first snapshot.
    if stats.block_fuel_kg.is_none() {
        stats.block_fuel_kg = Some(snap.fuel_total_kg);
    }

    let now = Utc::now();
    let prev_phase = stats.phase;
    let mut next_phase = prev_phase;
    let was_on_ground = stats.was_on_ground.unwrap_or(snap.on_ground);
    let was_brake = stats.was_parking_brake.unwrap_or(snap.parking_brake);

    // Match on a local Copy so the rest of the body is free to mutate `stats`.
    match prev_phase {
        FlightPhase::Boarding => {
            if was_brake && !snap.parking_brake && snap.on_ground {
                next_phase = FlightPhase::Pushback;
                stats.block_off_at = Some(now);
            }
        }
        FlightPhase::Pushback => {
            if snap.groundspeed_kt > 5.0 && snap.on_ground {
                next_phase = FlightPhase::TaxiOut;
            }
        }
        FlightPhase::TaxiOut => {
            if snap.groundspeed_kt > 40.0 && snap.on_ground {
                next_phase = FlightPhase::TakeoffRoll;
            }
        }
        FlightPhase::TakeoffRoll => {
            if was_on_ground && !snap.on_ground {
                next_phase = FlightPhase::Takeoff;
                stats.takeoff_at = Some(now);
                stats.takeoff_fuel_kg = Some(snap.fuel_total_kg);
                let zfw = snap.zfw_kg.unwrap_or(0.0);
                let weight = zfw as f64 + snap.fuel_total_kg as f64;
                if weight > 0.0 {
                    stats.takeoff_weight_kg = Some(weight);
                }
            }
        }
        FlightPhase::Takeoff => {
            if snap.altitude_agl_ft > 500.0 {
                next_phase = FlightPhase::Climb;
            }
        }
        FlightPhase::Climb | FlightPhase::Cruise => {
            if snap.vertical_speed_fpm < -300.0 {
                next_phase = FlightPhase::Descent;
            }
        }
        FlightPhase::Descent => {
            if snap.altitude_agl_ft < 5000.0 {
                next_phase = FlightPhase::Approach;
            }
        }
        FlightPhase::Approach => {
            if snap.altitude_agl_ft < 1500.0 {
                next_phase = FlightPhase::Final;
            }
        }
        FlightPhase::Final => {
            if !was_on_ground && snap.on_ground {
                next_phase = FlightPhase::Landing;
                stats.landing_at = Some(now);
                stats.landing_rate_fpm = Some(snap.vertical_speed_fpm);
                stats.landing_g_force = Some(snap.g_force);
                stats.landing_pitch_deg = Some(snap.pitch_deg);
                stats.landing_speed_kt = Some(snap.indicated_airspeed_kt);
                stats.landing_heading_deg = Some(snap.heading_deg_magnetic);
                stats.landing_fuel_kg = Some(snap.fuel_total_kg);
                let zfw = snap.zfw_kg.unwrap_or(0.0);
                let weight = zfw as f64 + snap.fuel_total_kg as f64;
                if weight > 0.0 {
                    stats.landing_weight_kg = Some(weight);
                }
            }
        }
        FlightPhase::Landing => {
            if snap.groundspeed_kt < 30.0 && snap.on_ground {
                next_phase = FlightPhase::TaxiIn;
            }
        }
        FlightPhase::TaxiIn => {
            if snap.parking_brake && snap.groundspeed_kt < 1.0 && snap.on_ground {
                next_phase = FlightPhase::BlocksOn;
                stats.block_on_at = Some(now);
            }
        }
        FlightPhase::BlocksOn
        | FlightPhase::Arrived
        | FlightPhase::PirepSubmitted
        | FlightPhase::Preflight => {}
    }

    stats.was_on_ground = Some(snap.on_ground);
    stats.was_parking_brake = Some(snap.parking_brake);

    if next_phase != prev_phase {
        stats.phase = next_phase;
        stats.transitions.push((now, next_phase));
        Some(next_phase)
    } else {
        None
    }
}

/// Build the custom-fields map sent in `POST /api/pireps/{id}/file`. Field
/// names follow the de-facto vmsACARS convention so VAs that already configured
/// fields for vmsACARS see them populate without any work.
fn build_pirep_fields(
    flight: &ActiveFlight,
    stats: &FlightStats,
) -> HashMap<String, String> {
    let mut f: HashMap<String, String> = HashMap::new();

    f.insert(
        "Source".into(),
        format!("CloudeAcars/{}", env!("CARGO_PKG_VERSION")),
    );
    f.insert("Departure Airport".into(), flight.dpt_airport.clone());
    f.insert("Arrival Airport".into(), flight.arr_airport.clone());

    if let Some(t) = stats.block_off_at {
        f.insert("Blocks Off Time".into(), t.to_rfc3339());
    }
    if let Some(t) = stats.takeoff_at {
        f.insert("Takeoff Time".into(), t.to_rfc3339());
    }
    if let Some(t) = stats.landing_at {
        f.insert("Landing Time".into(), t.to_rfc3339());
    }
    if let Some(t) = stats.block_on_at {
        f.insert("Blocks On Time".into(), t.to_rfc3339());
    }

    if let Some(w) = stats.takeoff_weight_kg {
        f.insert("Takeoff Weight".into(), format!("{:.0} kg", w));
    }
    if let Some(rate) = stats.landing_rate_fpm {
        // Negative on touchdown — preserve sign so VAs see e.g. -221 fpm.
        f.insert("Landing Rate".into(), format!("{:.0} fpm", rate));
    }
    if let Some(g) = stats.landing_g_force {
        f.insert("Landing G-Force".into(), format!("{:.2} G", g));
    }
    if let Some(p) = stats.landing_pitch_deg {
        f.insert("Landing Pitch".into(), format!("{:.1}°", p));
    }
    if let Some(s) = stats.landing_speed_kt {
        f.insert("Landing Speed".into(), format!("{:.0} kt", s));
    }
    if let Some(h) = stats.landing_heading_deg {
        f.insert("Landing Heading".into(), format!("{:03.0}°", h));
    }
    if let Some(w) = stats.landing_weight_kg {
        f.insert("Landing Weight".into(), format!("{:.0} kg", w));
    }
    if let Some(fuel) = stats.landing_fuel_kg {
        f.insert("Landing Fuel".into(), format!("{:.0} kg", fuel));
    }
    if let Some(b) = stats.block_fuel_kg {
        f.insert("Block Fuel".into(), format!("{:.0} kg", b));
    }
    if let (Some(b), Some(c)) = (stats.block_fuel_kg, stats.last_fuel_kg) {
        if b > c {
            f.insert("Fuel Used".into(), format!("{:.0} kg", b - c));
        }
    }

    // Computed durations.
    if let (Some(off), Some(on)) = (stats.block_off_at, stats.block_on_at) {
        f.insert(
            "Total Block Time".into(),
            humanize_duration_minutes((on - off).num_minutes()),
        );
    }
    if let (Some(takeoff), Some(landing)) = (stats.takeoff_at, stats.landing_at) {
        f.insert(
            "Total Flight Time".into(),
            humanize_duration_minutes((landing - takeoff).num_minutes()),
        );
    }
    if let (Some(land), Some(blocks_on)) = (stats.landing_at, stats.block_on_at) {
        f.insert(
            "Taxi In Time".into(),
            humanize_duration_minutes((blocks_on - land).num_minutes()),
        );
    }

    f
}

fn humanize_duration_minutes(minutes: i64) -> String {
    let m = minutes.max(0);
    let h = m / 60;
    let r = m % 60;
    if h == 0 {
        format!("{}m", r)
    } else {
        format!("{}h {:02}m", h, r)
    }
}

/// Build the human-readable summary that goes into the PIREP `notes` field —
/// a concise multi-line text that's always visible regardless of how the VA
/// configured custom fields.
fn build_pirep_notes(flight: &ActiveFlight, stats: &FlightStats) -> String {
    let mut lines = Vec::new();
    lines.push(format!(
        "{} {} → {}",
        flight.flight_number, flight.dpt_airport, flight.arr_airport
    ));
    if let Some(t) = stats.block_off_at {
        lines.push(format!("Blocks off: {}", t.to_rfc3339()));
    }
    if let Some(t) = stats.takeoff_at {
        lines.push(format!("Takeoff: {}", t.to_rfc3339()));
    }
    if let Some(t) = stats.landing_at {
        lines.push(format!("Landing: {}", t.to_rfc3339()));
    }
    if let Some(t) = stats.block_on_at {
        lines.push(format!("Blocks on: {}", t.to_rfc3339()));
    }
    if let Some(rate) = stats.landing_rate_fpm {
        lines.push(format!(
            "Touchdown: {:.0} fpm, {:.2} G, {:.1}° pitch, {:.0} kt",
            rate,
            stats.landing_g_force.unwrap_or(0.0),
            stats.landing_pitch_deg.unwrap_or(0.0),
            stats.landing_speed_kt.unwrap_or(0.0),
        ));
    }
    if let (Some(b), Some(c)) = (stats.block_fuel_kg, stats.last_fuel_kg) {
        if b > c {
            lines.push(format!("Fuel: {:.0} kg block / {:.0} kg used", b, b - c));
        }
    }
    lines.push(format!(
        "CloudeAcars {} ({} positions, {:.1} nm)",
        env!("CARGO_PKG_VERSION"),
        stats.position_count,
        stats.distance_nm
    ));
    lines.join("\n")
}

/// Map our internal `FlightPhase` to the phpVMS PirepStatus code we POST in
/// `update_pirep`. Some phases collapse to the same code (e.g. Climb and
/// Cruise both report ENR).
fn phase_to_status(phase: FlightPhase) -> Option<&'static str> {
    match phase {
        FlightPhase::Preflight | FlightPhase::Boarding => Some("BST"),
        FlightPhase::Pushback => Some("OFB"),
        FlightPhase::TaxiOut => Some("TXI"),
        FlightPhase::TakeoffRoll => Some("TKO"),
        FlightPhase::Takeoff => Some("TOF"),
        FlightPhase::Climb | FlightPhase::Cruise => Some("ENR"),
        FlightPhase::Descent => Some("TEN"),
        FlightPhase::Approach | FlightPhase::Final => Some("APP"),
        FlightPhase::Landing | FlightPhase::TaxiIn => Some("LAN"),
        FlightPhase::BlocksOn | FlightPhase::Arrived => Some("ARR"),
        FlightPhase::PirepSubmitted => None,
    }
}

fn snapshot_to_position(snap: &SimSnapshot) -> PositionEntry {
    PositionEntry {
        lat: snap.lat,
        lon: snap.lon,
        altitude: snap.altitude_msl_ft,
        altitude_agl: Some(snap.altitude_agl_ft),
        heading: Some(snap.heading_deg_magnetic),
        gs: Some(snap.groundspeed_kt),
        vs: Some(snap.vertical_speed_fpm),
        ias: Some(snap.indicated_airspeed_kt),
        log: None,
        sim_time: snap.timestamp.to_rfc3339(),
    }
}

// ---- Simulator selection + status ----

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct SimConfig {
    #[serde(default)]
    kind: SimKind,
}

fn sim_config_path(app: &AppHandle) -> Result<PathBuf, UiError> {
    app.path()
        .app_config_dir()
        .map(|dir| dir.join(SIM_CONFIG_FILE))
        .map_err(|e| UiError::new("config_path", e.to_string()))
}

fn read_sim_config(app: &AppHandle) -> SimConfig {
    let Ok(path) = sim_config_path(app) else {
        return SimConfig::default();
    };
    if !path.exists() {
        return SimConfig::default();
    }
    match std::fs::read(&path).map(|b| serde_json::from_slice::<SimConfig>(&b)) {
        Ok(Ok(cfg)) => cfg,
        _ => SimConfig::default(),
    }
}

fn write_sim_config(app: &AppHandle, cfg: &SimConfig) -> Result<(), UiError> {
    let path = sim_config_path(app)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| UiError::new("config_write", e.to_string()))?;
    }
    let json = serde_json::to_vec_pretty(cfg)
        .map_err(|e| UiError::new("config_serialize", e.to_string()))?;
    std::fs::write(&path, json).map_err(|e| UiError::new("config_write", e.to_string()))
}

/// Apply the selected kind to the MSFS adapter (start / stop / no-op).
/// X-Plane kinds are accepted as a setting but the X-Plane adapter is Phase 2;
/// for now we just stop the MSFS adapter and let the UI display the "coming
/// soon" state.
fn apply_sim_kind(_state: &tauri::State<'_, AppState>, _kind: SimKind) {
    #[cfg(target_os = "windows")]
    {
        let mut adapter = _state.msfs.lock().expect("msfs lock");
        if _kind.is_msfs() {
            adapter.start(_kind);
        } else {
            adapter.stop();
        }
    }
}

#[derive(Serialize, Default)]
pub struct SimStatus {
    /// "disconnected" | "connecting" | "connected"
    state: String,
    /// User-selected sim ("off" | "msfs2020" | "msfs2024" | "xplane11" | "xplane12").
    kind: String,
    snapshot: Option<SimSnapshot>,
    last_error: Option<String>,
    /// Whether the selected kind is actually implemented in this build.
    available: bool,
}

fn kind_str(kind: SimKind) -> &'static str {
    match kind {
        SimKind::Off => "off",
        SimKind::Msfs2020 => "msfs2020",
        SimKind::Msfs2024 => "msfs2024",
        SimKind::XPlane11 => "xplane11",
        SimKind::XPlane12 => "xplane12",
    }
}

/// `GET` the persisted sim selection.
#[tauri::command]
fn sim_get_kind(app: AppHandle) -> String {
    kind_str(read_sim_config(&app).kind).to_string()
}

/// Persist a new sim selection AND apply it to the running adapter.
/// Accepts: "off" | "msfs2020" | "msfs2024" | "xplane11" | "xplane12".
#[tauri::command]
fn sim_set_kind(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    kind: String,
) -> Result<(), UiError> {
    let parsed = match kind.as_str() {
        "off" => SimKind::Off,
        "msfs2020" => SimKind::Msfs2020,
        "msfs2024" => SimKind::Msfs2024,
        "xplane11" => SimKind::XPlane11,
        "xplane12" => SimKind::XPlane12,
        _ => return Err(UiError::new("invalid_sim_kind", format!("unknown kind: {kind}"))),
    };
    write_sim_config(&app, &SimConfig { kind: parsed })?;
    apply_sim_kind(&state, parsed);
    tracing::info!(?parsed, "sim kind selected");
    Ok(())
}

#[tauri::command]
fn sim_status(app: AppHandle, _state: tauri::State<'_, AppState>) -> SimStatus {
    let kind = read_sim_config(&app).kind;
    #[cfg(target_os = "windows")]
    {
        let adapter = _state.msfs.lock().expect("msfs lock");
        let (state_str, last_error) = if kind.is_msfs() {
            let s = match adapter.state() {
                sim_msfs::ConnectionState::Disconnected => "disconnected",
                sim_msfs::ConnectionState::Connecting => "connecting",
                sim_msfs::ConnectionState::Connected => "connected",
            };
            (s, adapter.last_error())
        } else if kind.is_xplane() {
            ("disconnected", Some("X-Plane support arrives in Phase 2".into()))
        } else {
            ("disconnected", None)
        };
        let snapshot = if kind.is_msfs() {
            adapter.snapshot()
        } else {
            None
        };
        SimStatus {
            state: state_str.into(),
            kind: kind_str(kind).into(),
            snapshot,
            last_error,
            available: kind.is_msfs() || kind == SimKind::Off,
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        let last_error = if kind.is_msfs() {
            Some("MSFS adapter is Windows-only".into())
        } else if kind.is_xplane() {
            Some("X-Plane support arrives in Phase 2".into())
        } else {
            None
        };
        SimStatus {
            state: "disconnected".into(),
            kind: kind_str(kind).into(),
            snapshot: None,
            last_error,
            available: kind == SimKind::Off,
        }
    }
}

/// On login or session restore, check the on-disk active-flight file. If it's
/// recent enough, recreate the in-memory ActiveFlight and restart position
/// streaming — picks up exactly where the previous run left off.
fn try_resume_flight(app: &AppHandle, state: &tauri::State<'_, AppState>, client: &Client) {
    let Some(persisted) = read_persisted_flight(app) else {
        return;
    };
    // Drop sessions that are clearly stale (e.g. a flight from days ago) so
    // we don't keep flogging a long-dead PIREP forever.
    let age = Utc::now() - persisted.started_at;
    if age > chrono::Duration::hours(RESUME_MAX_AGE_HOURS) {
        tracing::info!(
            pirep_id = %persisted.pirep_id,
            age_hours = age.num_hours(),
            "discarding stale persisted flight"
        );
        clear_persisted_flight(app);
        return;
    }

    {
        let guard = state.active_flight.lock().expect("active_flight lock");
        if guard.is_some() {
            tracing::warn!("active flight already in memory, skipping resume");
            return;
        }
    }

    tracing::info!(
        pirep_id = %persisted.pirep_id,
        age_minutes = age.num_minutes(),
        "resuming in-progress flight"
    );

    let flight = Arc::new(ActiveFlight {
        pirep_id: persisted.pirep_id.clone(),
        bid_id: persisted.bid_id,
        started_at: persisted.started_at,
        flight_number: persisted.flight_number.clone(),
        dpt_airport: persisted.dpt_airport.clone(),
        arr_airport: persisted.arr_airport.clone(),
        fares: persisted.fares.clone(),
        stats: Mutex::new(FlightStats::new()),
        stop: AtomicBool::new(false),
    });

    {
        let mut guard = state.active_flight.lock().expect("active_flight lock");
        *guard = Some(Arc::clone(&flight));
    }
    spawn_position_streamer(app.clone(), flight, client.clone());
}

// ---- Bootstrap ----

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,cloudeacars=debug"));
    let _ = tracing_subscriber::fmt().with_env_filter(filter).try_init();
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    init_tracing();
    tracing::info!(version = env!("CARGO_PKG_VERSION"), "CloudeAcars starting");

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            app_info,
            phpvms_login,
            phpvms_logout,
            phpvms_load_session,
            phpvms_get_bids,
            sim_get_kind,
            sim_set_kind,
            sim_status,
            airport_get,
            flight_status,
            flight_start,
            flight_end,
            flight_cancel,
            flight_forget,
        ])
        .run(tauri::generate_context!())
        .expect("error while running CloudeAcars");
}
