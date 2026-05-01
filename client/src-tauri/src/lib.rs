//! CloudeAcars — Tauri application root.
//!
//! Wires together the internal crates and exposes Tauri commands to the UI.
//! Phase 1: only `app_info` for now; further commands grow with each phase.

use tracing_subscriber::EnvFilter;

#[derive(serde::Serialize)]
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
        .invoke_handler(tauri::generate_handler![app_info])
        .run(tauri::generate_context!())
        .expect("error while running CloudeAcars");
}
