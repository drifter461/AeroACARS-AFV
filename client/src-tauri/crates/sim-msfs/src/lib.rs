//! MSFS 2020 / MSFS 2024 simulator adapter — **SimConnect only, never FSUIPC**.
//!
//! See ADR-0002 in `docs/decisions/0002-msfs-simconnect-only.md`.
//!
//! Reference docs: <https://docs.flightsimulator.com/html/Programming_Tools/SimConnect/SimConnect_SDK.htm>
//!
//! Status: Phase 1 stub. The Windows-only implementation lands once the SimConnect
//! crate / `bindgen` decision is taken.

#![allow(dead_code)]

#[cfg(target_os = "windows")]
pub struct MsfsAdapter {
    // To be filled in: SimConnect handle, dispatch loop sender, latest snapshot, etc.
    _placeholder: (),
}

#[cfg(target_os = "windows")]
impl MsfsAdapter {
    pub fn new() -> Self {
        Self { _placeholder: () }
    }
}

// On non-Windows platforms the adapter doesn't exist — referenced via cfg gates in the app.
#[cfg(not(target_os = "windows"))]
pub struct MsfsAdapter;
