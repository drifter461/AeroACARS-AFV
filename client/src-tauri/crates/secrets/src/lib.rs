//! OS keyring wrapper for secrets (phpVMS API key, future tokens).
//!
//! Backends used by the `keyring` crate:
//!   * Windows: Credential Manager
//!   * macOS:   Keychain
//!   * Linux:   Secret Service (libsecret)
//!
//! See requirements spec §29 — "API-Key sicher speichern, keine Passwörter im Klartext".

#![allow(dead_code)]

use thiserror::Error;

const SERVICE: &str = "CloudeAcars";

#[derive(Debug, Error)]
pub enum SecretError {
    #[error("keyring error: {0}")]
    Keyring(#[from] keyring::Error),
}

pub fn store_api_key(account: &str, api_key: &str) -> Result<(), SecretError> {
    let entry = keyring::Entry::new(SERVICE, account)?;
    entry.set_password(api_key)?;
    Ok(())
}

pub fn load_api_key(account: &str) -> Result<Option<String>, SecretError> {
    let entry = keyring::Entry::new(SERVICE, account)?;
    match entry.get_password() {
        Ok(s) => Ok(Some(s)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

pub fn delete_api_key(account: &str) -> Result<(), SecretError> {
    let entry = keyring::Entry::new(SERVICE, account)?;
    match entry.delete_credential() {
        Ok(()) => Ok(()),
        Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(e.into()),
    }
}
