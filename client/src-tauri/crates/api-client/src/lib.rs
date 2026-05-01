//! phpVMS 7 HTTP API client.
//!
//! Talks to:
//!   * phpVMS Core API (users, bids, flights, fleet, PIREP file, ACARS positions)
//!   * CloudeAcars phpVMS module (config, version, heartbeat, landing extras) — Phase 4
//!
//! Authentication: phpVMS API key sent via the `X-API-Key` header (phpVMS standard).
//! All requests advertise `User-Agent: CloudeAcars/<version>` so the server can identify us.

#![allow(dead_code)] // Phase 1: only auth + profile implemented; others stubbed.

use std::time::Duration;

use reqwest::{header, Client as HttpClient, StatusCode};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use url::Url;

/// Default request timeout. ACARS position posts are time-sensitive but
/// also tolerant of variable network conditions for VAs hosted abroad.
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(20);

#[derive(Debug, Error)]
pub enum ApiError {
    #[error("invalid base URL: {0}")]
    InvalidUrl(String),
    #[error("network error: {0}")]
    Network(String),
    #[error("authentication failed (HTTP 401) — check API key")]
    Unauthenticated,
    #[error("forbidden (HTTP 403) — account may lack permissions")]
    Forbidden,
    #[error("not found (HTTP 404) — endpoint missing on this phpVMS site")]
    NotFound,
    #[error("rate limited (HTTP 429), retry after {retry_after_seconds}s")]
    RateLimited { retry_after_seconds: u64 },
    #[error("server error (HTTP {status}): {body}")]
    Server { status: u16, body: String },
    #[error("unexpected response shape: {0}")]
    BadResponse(String),
}

impl ApiError {
    /// Stable identifier surfaced to the UI for i18n key lookup.
    /// The frontend maps these to localized error messages.
    pub fn code(&self) -> &'static str {
        match self {
            ApiError::InvalidUrl(_) => "invalid_url",
            ApiError::Network(_) => "network",
            ApiError::Unauthenticated => "unauthenticated",
            ApiError::Forbidden => "forbidden",
            ApiError::NotFound => "not_found",
            ApiError::RateLimited { .. } => "rate_limited",
            ApiError::Server { .. } => "server",
            ApiError::BadResponse(_) => "bad_response",
        }
    }
}

impl From<reqwest::Error> for ApiError {
    fn from(err: reqwest::Error) -> Self {
        ApiError::Network(err.to_string())
    }
}

/// Connection details for a phpVMS site.
#[derive(Clone, Debug)]
pub struct Connection {
    base_url: Url,
    api_key: String,
}

impl Connection {
    pub fn new(base_url: &str, api_key: impl Into<String>) -> Result<Self, ApiError> {
        let trimmed = base_url.trim().trim_end_matches('/');
        let url = Url::parse(trimmed).map_err(|_| ApiError::InvalidUrl(trimmed.into()))?;
        if !matches!(url.scheme(), "http" | "https") {
            return Err(ApiError::InvalidUrl(format!(
                "URL must be http(s), got '{}'",
                url.scheme()
            )));
        }
        Ok(Self {
            base_url: url,
            api_key: api_key.into(),
        })
    }

    pub fn base_url(&self) -> &str {
        self.base_url.as_str()
    }
}

/// Subset of `GET /api/user` we need in Phase 1.
/// phpVMS returns more fields; we deserialize only what we use.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Profile {
    pub id: i64,
    pub pilot_id: i64,
    pub name: String,
    pub email: Option<String>,
    pub airline_id: Option<i64>,
    pub curr_airport_id: Option<String>,
    pub home_airport_id: Option<String>,
    #[serde(default)]
    pub airline: Option<Airline>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Airline {
    pub id: i64,
    pub icao: String,
    pub iata: Option<String>,
    pub name: String,
}

/// phpVMS resource responses are wrapped: `{ "data": {...} }`.
#[derive(Deserialize)]
struct DataEnvelope<T> {
    data: T,
}

/// A reusable client. `Clone` is cheap because the inner reqwest client is
/// `Arc`-backed and `Connection` only holds a URL + API key string.
#[derive(Clone)]
pub struct Client {
    http: HttpClient,
    conn: Connection,
}

impl Client {
    pub fn new(conn: Connection) -> Result<Self, ApiError> {
        let user_agent = format!("CloudeAcars/{}", env!("CARGO_PKG_VERSION"));
        let http = HttpClient::builder()
            .user_agent(user_agent)
            .timeout(DEFAULT_TIMEOUT)
            .build()
            .map_err(ApiError::from)?;
        Ok(Self { http, conn })
    }

    pub fn connection(&self) -> &Connection {
        &self.conn
    }

    fn endpoint(&self, path: &str) -> Result<Url, ApiError> {
        let path = path.trim_start_matches('/');
        let joined = format!("{}/{}", self.conn.base_url.as_str().trim_end_matches('/'), path);
        Url::parse(&joined).map_err(|_| ApiError::InvalidUrl(joined))
    }

    /// `GET /api/user` — current user (the pilot the API key belongs to).
    /// Used as the auth probe during login and to populate the dashboard.
    pub async fn get_profile(&self) -> Result<Profile, ApiError> {
        let url = self.endpoint("/api/user")?;
        let response = self
            .http
            .get(url)
            .header("X-API-Key", &self.conn.api_key)
            .header(header::ACCEPT, "application/json")
            .send()
            .await?;

        let status = response.status();
        match status {
            StatusCode::OK => {}
            StatusCode::UNAUTHORIZED => return Err(ApiError::Unauthenticated),
            StatusCode::FORBIDDEN => return Err(ApiError::Forbidden),
            StatusCode::NOT_FOUND => return Err(ApiError::NotFound),
            StatusCode::TOO_MANY_REQUESTS => {
                let retry_after_seconds = response
                    .headers()
                    .get(header::RETRY_AFTER)
                    .and_then(|v| v.to_str().ok())
                    .and_then(|s| s.parse::<u64>().ok())
                    .unwrap_or(60);
                return Err(ApiError::RateLimited { retry_after_seconds });
            }
            s if s.is_server_error() => {
                let body = response.text().await.unwrap_or_default();
                return Err(ApiError::Server { status: s.as_u16(), body });
            }
            s => {
                let body = response.text().await.unwrap_or_default();
                return Err(ApiError::Server { status: s.as_u16(), body });
            }
        }

        let envelope: DataEnvelope<Profile> = response
            .json()
            .await
            .map_err(|e| ApiError::BadResponse(format!("JSON decode failed for /api/user: {e}")))?;

        Ok(envelope.data)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_non_http_scheme() {
        let err = Connection::new("ftp://example.com", "k").unwrap_err();
        assert!(matches!(err, ApiError::InvalidUrl(_)));
    }

    #[test]
    fn accepts_https() {
        Connection::new("https://example.com", "k").unwrap();
    }

    #[test]
    fn accepts_http_localhost() {
        Connection::new("http://localhost:8000", "k").unwrap();
    }

    #[test]
    fn strips_trailing_slash() {
        let c = Connection::new("https://example.com/", "k").unwrap();
        assert!(!c.base_url().ends_with("//"));
    }
}
