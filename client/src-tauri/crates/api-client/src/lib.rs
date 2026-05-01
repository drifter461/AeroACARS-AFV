//! phpVMS 7 HTTP API client.
//!
//! Talks to:
//!   * phpVMS Core API (users, bids, flights, fleet, PIREP file, ACARS positions)
//!   * CloudeAcars phpVMS module (config, version, heartbeat, landing extras)
//!
//! Phase 1 deliverable: login + profile fetch + bids list + simple flight search.
//! Authentication: phpVMS API key sent via the `X-API-Key` header.

#![allow(dead_code)] // Phase 1 stub — fields/types added incrementally.

use thiserror::Error;
use url::Url;

#[derive(Debug, Error)]
pub enum ApiError {
    #[error("invalid base URL: {0}")]
    InvalidUrl(String),
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("API returned status {status}: {body}")]
    Status { status: u16, body: String },
    #[error("authentication failed (HTTP 401)")]
    Unauthenticated,
    #[error("rate limited (HTTP 429), retry after {retry_after_seconds}s")]
    RateLimited { retry_after_seconds: u64 },
}

/// Connection details for a phpVMS site.
#[derive(Clone, Debug)]
pub struct Connection {
    pub base_url: Url,
    pub api_key: String,
    pub user_agent: String,
}

impl Connection {
    pub fn new(base_url: &str, api_key: impl Into<String>) -> Result<Self, ApiError> {
        let url = Url::parse(base_url).map_err(|_| ApiError::InvalidUrl(base_url.into()))?;
        Ok(Self {
            base_url: url,
            api_key: api_key.into(),
            user_agent: format!("CloudeAcars/{}", env!("CARGO_PKG_VERSION")),
        })
    }
}

// TODO(phase-1): implement
//   - GET /api/user                       -> Profile
//   - GET /api/user/bids                  -> Bids
//   - GET /api/fleet                      -> Fleet
//   - GET /api/cloudeacars/config         -> module config
//   - POST /api/pireps/prefile + /file    -> submit PIREP
//   - POST /api/acars/{id}/position       -> live position stream
