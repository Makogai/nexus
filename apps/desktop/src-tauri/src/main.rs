// Prevents an additional console window on Windows in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // First, before anything else runs: production builds drop webview debugging overrides (threat T18).
    if let Err(message) = nexus_desktop_lib::webview_guard::harden() {
        nexus_desktop_lib::webview_guard::refuse_to_start(&message);
    }
    nexus_desktop_lib::run()
}
