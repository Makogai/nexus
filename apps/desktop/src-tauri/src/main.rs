// Prevents an additional console window on Windows in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // First, before anything else runs: production builds drop webview debugging overrides (threat T18).
    if let Err(message) = nexus_desktop_lib::webview_guard::harden() {
        nexus_desktop_lib::webview_guard::refuse_to_start(&message);
    }
    // Installed-app verification without a window (ADR-0033); exits before the app (and its single-instance lock).
    if let Some(out) = nexus_desktop_lib::self_check::requested() {
        std::process::exit(nexus_desktop_lib::self_check::run(&out));
    }
    nexus_desktop_lib::run()
}
