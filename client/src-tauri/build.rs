fn main() {
    // v0.9.0 (#GlitchTip): Damit `option_env!("AEROACARS_SENTRY_DSN")` in
    // `src/sentry_init.rs` zur Build-Zeit ausgewertet wird, muss Cargo
    // bei Aenderung des env neu kompilieren. Sonst bleibt der eingebrannte
    // Wert aus dem ersten Build cached.
    println!("cargo:rerun-if-env-changed=AEROACARS_SENTRY_DSN");
    tauri_build::build()
}
