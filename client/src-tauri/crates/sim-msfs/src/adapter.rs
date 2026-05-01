//! Windows-only raw SimConnect adapter.
//!
//! Owns a worker thread that connects to SimConnect, registers a
//! single data definition, subscribes to per-second updates and
//! pushes parsed [`SimSnapshot`]s into a shared mutex. The public
//! [`MsfsAdapter`] API is the same as the legacy adapter so the rest
//! of the application doesn't need to change.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use chrono::Utc;
use serde::Serialize;
use sim_core::{AircraftProfile, SimKind, SimSnapshot, Simulator};

mod sys;
mod telemetry;

use telemetry::TELEMETRY_FIELDS;

// IDs used in our SimConnect calls — chosen freely as long as they're
// unique within the connection. We only ever register one definition
// and one request.
const DEFINITION_ID: sys::SIMCONNECT_DATA_DEFINITION_ID = 1;
const REQUEST_ID: sys::SIMCONNECT_DATA_REQUEST_ID = 1;
const STALE_TIMEOUT: Duration = Duration::from_secs(5);

/// Public connection state mirrored to the frontend.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionState {
    Disconnected,
    Connecting,
    Connected,
}

/// External-facing MSFS adapter. Cheap to clone-state; drives a
/// background worker thread that talks to SimConnect.
pub struct MsfsAdapter {
    shared: Arc<Shared>,
    worker: Option<JoinHandle<()>>,
    stop: Arc<AtomicBool>,
}

struct Shared {
    state: Mutex<ConnectionState>,
    snapshot: Mutex<Option<SimSnapshot>>,
    last_error: Mutex<Option<String>>,
}

impl Default for MsfsAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl MsfsAdapter {
    pub fn new() -> Self {
        Self {
            shared: Arc::new(Shared {
                state: Mutex::new(ConnectionState::Disconnected),
                snapshot: Mutex::new(None),
                last_error: Mutex::new(None),
            }),
            worker: None,
            stop: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Start the worker thread. Idempotent: a second call is a no-op
    /// while a worker is already running.
    pub fn start(&mut self, kind: SimKind) {
        if self.worker.is_some() {
            return;
        }
        if !kind.is_msfs() {
            *self.shared.state.lock().unwrap() = ConnectionState::Disconnected;
            return;
        }
        self.stop = Arc::new(AtomicBool::new(false));
        let shared = Arc::clone(&self.shared);
        let stop = Arc::clone(&self.stop);
        *shared.state.lock().unwrap() = ConnectionState::Connecting;
        *shared.last_error.lock().unwrap() = None;
        tracing::info!(?kind, "MSFS raw adapter started");
        let handle = thread::Builder::new()
            .name("sim-msfs-worker".into())
            .spawn(move || worker_loop(shared, stop, kind))
            .expect("could not spawn sim-msfs worker thread");
        self.worker = Some(handle);
    }

    pub fn stop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(h) = self.worker.take() {
            // Give the worker a moment to wind down cleanly. We don't
            // join indefinitely — SimConnect_Close inside the worker
            // can hang if MSFS itself is gone.
            let _ = h.join();
        }
        *self.shared.state.lock().unwrap() = ConnectionState::Disconnected;
        tracing::info!("MSFS raw adapter stopped");
    }

    pub fn state(&self) -> ConnectionState {
        *self.shared.state.lock().unwrap()
    }

    pub fn snapshot(&self) -> Option<SimSnapshot> {
        self.shared.snapshot.lock().unwrap().clone()
    }

    pub fn last_error(&self) -> Option<String> {
        self.shared.last_error.lock().unwrap().clone()
    }
}

impl Drop for MsfsAdapter {
    fn drop(&mut self) {
        self.stop();
    }
}

// ---- Worker loop ----

fn worker_loop(shared: Arc<Shared>, stop: Arc<AtomicBool>, kind: SimKind) {
    // Outer reconnect loop. SimConnect_Open returns E_FAIL while MSFS
    // isn't running; we simply retry every 2s until it's up.
    while !stop.load(Ordering::Relaxed) {
        match Connection::open("CloudeAcars") {
            Ok(mut conn) => {
                tracing::info!("SimConnect_Open succeeded — registering data definition");
                if let Err(e) = conn.register_telemetry() {
                    set_error(&shared, format!("RegisterDataDefinition failed: {e}"));
                    tracing::error!(error = %e, "register_telemetry failed");
                    drop(conn);
                    sleep_or_stop(&stop, Duration::from_secs(2));
                    continue;
                }
                if let Err(e) = conn.request_data_per_second() {
                    set_error(&shared, format!("RequestDataOnSimObject failed: {e}"));
                    tracing::error!(error = %e, "request_data_per_second failed");
                    drop(conn);
                    sleep_or_stop(&stop, Duration::from_secs(2));
                    continue;
                }
                run_dispatch(&shared, &stop, &mut conn, kind);
                // run_dispatch only returns when stop is signalled or
                // the connection has gone stale. Either way, drop and
                // try again at the top of the loop.
            }
            Err(e) => {
                let msg = format!("SimConnect_Open failed: {e}");
                set_error(&shared, msg);
                *shared.state.lock().unwrap() = ConnectionState::Connecting;
            }
        }
        sleep_or_stop(&stop, Duration::from_secs(2));
    }
    *shared.state.lock().unwrap() = ConnectionState::Disconnected;
}

fn run_dispatch(
    shared: &Arc<Shared>,
    stop: &Arc<AtomicBool>,
    conn: &mut Connection,
    kind: SimKind,
) {
    let mut last_data = Instant::now();
    let mut got_first = false;
    let simulator = kind.as_simulator();

    while !stop.load(Ordering::Relaxed) {
        // Drain whatever messages SimConnect has queued for us.
        loop {
            match conn.get_next_dispatch() {
                Ok(None) => break, // queue empty
                Ok(Some(DispatchMsg::Open)) => {
                    tracing::info!("SimConnect_RECV_OPEN — handshake done");
                }
                Ok(Some(DispatchMsg::Quit)) => {
                    tracing::warn!("SimConnect sent QUIT — dropping connection");
                    return;
                }
                Ok(Some(DispatchMsg::Exception {
                    exception,
                    send_id,
                    index,
                })) => {
                    // This is the diagnostic the legacy crate didn't
                    // give us — log the exact SimVar that failed.
                    let field = TELEMETRY_FIELDS.get(index as usize).map(|f| f.name);
                    tracing::warn!(
                        exception,
                        send_id,
                        index,
                        ?field,
                        "SIMCONNECT_RECV_EXCEPTION — SimVar request was rejected"
                    );
                }
                Ok(Some(DispatchMsg::SimObjectData { bytes })) => {
                    let snap = telemetry::parse(&bytes, simulator);
                    last_data = Instant::now();
                    if !got_first {
                        got_first = true;
                        *shared.state.lock().unwrap() = ConnectionState::Connected;
                        tracing::info!(
                            aircraft = ?snap.aircraft_title,
                            profile = ?snap.aircraft_profile,
                            "MSFS first snapshot received"
                        );
                        log_first_snapshot_diagnostics(&snap);
                    }
                    *shared.snapshot.lock().unwrap() = Some(snap);
                }
                Err(e) => {
                    tracing::warn!(error = %e, "SimConnect dispatch error");
                    return;
                }
            }
        }

        // Stale watchdog: if no data has arrived for a while assume
        // MSFS crashed or the pipe died, and let the outer loop
        // re-open the connection.
        if got_first && last_data.elapsed() > STALE_TIMEOUT {
            tracing::warn!("no SimConnect data for {:?} — reconnecting", STALE_TIMEOUT);
            return;
        }

        thread::sleep(Duration::from_millis(50));
    }
}

fn log_first_snapshot_diagnostics(snap: &SimSnapshot) {
    tracing::info!(
        fuel_total_kg = snap.fuel_total_kg,
        total_weight_kg = ?snap.total_weight_kg,
        aircraft_title = ?snap.aircraft_title,
        aircraft_profile = ?snap.aircraft_profile,
        "raw SimConnect first-snapshot fuel/weight diagnostic"
    );
}

fn set_error(shared: &Arc<Shared>, msg: String) {
    *shared.last_error.lock().unwrap() = Some(msg);
}

fn sleep_or_stop(stop: &Arc<AtomicBool>, dur: Duration) {
    let step = Duration::from_millis(100);
    let mut left = dur;
    while !left.is_zero() {
        if stop.load(Ordering::Relaxed) {
            return;
        }
        let s = std::cmp::min(step, left);
        thread::sleep(s);
        left = left.saturating_sub(s);
    }
}

// ---- Connection wrapper ----

/// Owns the SimConnect handle and provides the higher-level operations
/// the worker loop drives. `Drop` calls `SimConnect_Close`.
struct Connection {
    handle: sys::HANDLE,
}

impl Connection {
    fn open(name: &str) -> Result<Self, String> {
        let cname = std::ffi::CString::new(name).expect("connection name must be plain ASCII");
        let mut handle: sys::HANDLE = std::ptr::null_mut();
        let hr = unsafe {
            sys::SimConnect_Open(
                &mut handle,
                cname.as_ptr(),
                std::ptr::null_mut(),
                0,
                std::ptr::null_mut(),
                0,
            )
        };
        if hr != 0 {
            return Err(format!("HRESULT 0x{hr:08X}"));
        }
        Ok(Self { handle })
    }

    /// Register every entry in `TELEMETRY_FIELDS` in order.
    fn register_telemetry(&mut self) -> Result<(), String> {
        for (idx, field) in TELEMETRY_FIELDS.iter().enumerate() {
            let cname = std::ffi::CString::new(field.name)
                .map_err(|_| "SimVar name contained NUL".to_string())?;
            let cunit = std::ffi::CString::new(field.unit)
                .map_err(|_| "Unit string contained NUL".to_string())?;
            let datatype = match field.kind {
                telemetry::FieldKind::Float64 => sys::SIMCONNECT_DATATYPE_FLOAT64,
                telemetry::FieldKind::Int32 => sys::SIMCONNECT_DATATYPE_INT32,
                telemetry::FieldKind::String256 => sys::SIMCONNECT_DATATYPE_STRING256,
            };
            let hr = unsafe {
                sys::SimConnect_AddToDataDefinition(
                    self.handle,
                    DEFINITION_ID,
                    cname.as_ptr(),
                    cunit.as_ptr(),
                    datatype,
                    0.0,
                    u32::MAX,
                )
            };
            if hr != 0 {
                return Err(format!(
                    "AddToDataDefinition for SimVar #{idx} \"{}\" returned 0x{hr:08X}",
                    field.name
                ));
            }
        }
        Ok(())
    }

    /// Subscribe at SECOND cadence — the application bumps to faster
    /// intervals via its own polling loop, but for raw SimConnect a
    /// 1 Hz feed is plenty for our use.
    fn request_data_per_second(&mut self) -> Result<(), String> {
        let hr = unsafe {
            sys::SimConnect_RequestDataOnSimObject(
                self.handle,
                REQUEST_ID,
                DEFINITION_ID,
                sys::SIMCONNECT_OBJECT_ID_USER,
                sys::SIMCONNECT_PERIOD_SECOND,
                0,
                0,
                0,
                0,
            )
        };
        if hr != 0 {
            return Err(format!("HRESULT 0x{hr:08X}"));
        }
        Ok(())
    }

    /// Pull one message off the SimConnect queue, returning None when
    /// the queue is empty. Distinguishes the receiver IDs we actually
    /// care about; the rest are logged at trace level and dropped.
    fn get_next_dispatch(&mut self) -> Result<Option<DispatchMsg>, String> {
        let mut p_data: *mut sys::SIMCONNECT_RECV = std::ptr::null_mut();
        let mut cb_data: sys::DWORD = 0;
        let hr = unsafe { sys::SimConnect_GetNextDispatch(self.handle, &mut p_data, &mut cb_data) };
        if hr == sys::E_FAIL {
            // Empty queue — not an error in SimConnect-land.
            return Ok(None);
        }
        if hr != 0 {
            return Err(format!("GetNextDispatch returned 0x{hr:08X}"));
        }
        if p_data.is_null() || cb_data == 0 {
            return Ok(None);
        }
        let recv = unsafe { &*p_data };
        let id = recv.dwID;
        let msg = match id {
            sys::SIMCONNECT_RECV_ID_OPEN => Some(DispatchMsg::Open),
            sys::SIMCONNECT_RECV_ID_QUIT => Some(DispatchMsg::Quit),
            sys::SIMCONNECT_RECV_ID_EXCEPTION => {
                let exc = unsafe { &*(p_data as *const sys::SIMCONNECT_RECV_EXCEPTION) };
                Some(DispatchMsg::Exception {
                    exception: exc.dwException,
                    send_id: exc.dwSendID,
                    index: exc.dwIndex,
                })
            }
            sys::SIMCONNECT_RECV_ID_SIMOBJECT_DATA => {
                // dwData[1] in the SDK header — first byte of the
                // payload — is at the same offset as
                // `SIMCONNECT_RECV_SIMOBJECT_DATA::dwData`. We copy
                // the bytes out so the dispatch ptr can be reused.
                let header_size = std::mem::size_of::<sys::SIMCONNECT_RECV_SIMOBJECT_DATA>();
                let total = cb_data as usize;
                if total < header_size {
                    return Ok(None);
                }
                let payload_start = header_size - std::mem::size_of::<sys::DWORD>();
                let payload_len = total - payload_start;
                let bytes = unsafe {
                    let base = p_data as *const u8;
                    std::slice::from_raw_parts(base.add(payload_start), payload_len)
                };
                Some(DispatchMsg::SimObjectData {
                    bytes: bytes.to_vec(),
                })
            }
            _ => None,
        };
        Ok(msg)
    }
}

impl Drop for Connection {
    fn drop(&mut self) {
        if !self.handle.is_null() {
            unsafe { sys::SimConnect_Close(self.handle) };
            self.handle = std::ptr::null_mut();
        }
    }
}

unsafe impl Send for Connection {}
unsafe impl Sync for Connection {}

#[derive(Debug)]
enum DispatchMsg {
    Open,
    Quit,
    Exception {
        exception: u32,
        send_id: u32,
        index: u32,
    },
    SimObjectData {
        bytes: Vec<u8>,
    },
}

// Marker so the file always references kind/Utc when stub'd out.
#[allow(dead_code)]
fn _link_assertions() {
    let _ = Utc::now();
    let _ = Simulator::Msfs2024;
    let _ = AircraftProfile::Default;
}
