//! X-Plane 11 / X-Plane 12 adapter.
//!
//! Architecture (see ADR-0004): we ship our own XPLM plugin (`xplane-plugin/`) inside the
//! installer. The plugin runs in-process inside X-Plane and pushes telemetry over UDP
//! loopback to this adapter. Default port: 49021. Wire format: CBOR-encoded `SimSnapshot`.
//!
//! Status: Phase 2 — only stubs and constants in Phase 1.

#![allow(dead_code)]

/// Default UDP port the XPLM plugin sends snapshots to.
pub const DEFAULT_UDP_PORT: u16 = 49021;

pub struct XPlaneAdapter {
    _placeholder: (),
}

impl XPlaneAdapter {
    pub fn new() -> Self {
        Self { _placeholder: () }
    }
}
