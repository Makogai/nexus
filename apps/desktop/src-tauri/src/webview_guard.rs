//! Production hardening of the system webview (threat T18, ADR-0015).
//!
//! WebView2 honours `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS` (and related variables) from the process environment
//! and they take precedence over anything the application passes, so an inherited `--remote-debugging-port` would
//! open a DevTools endpoint that any local process could use to drive NEXUS's privileged IPC. WebKitGTK has
//! equivalent inspector variables.
//!
//! * **Production** (embedded frontend, no test feature): every webview override variable is removed from the
//!   process environment before any webview exists, and a WebView2 registry policy that enables remote debugging
//!   makes NEXUS refuse to start.
//! * **E2E testing**: only with the explicit Cargo feature `e2e-webview-debugging` (never enabled for distributed
//!   builds) — the overrides are left in place so automated tests can attach.
//! * **Dev server** builds (`tauri dev`, frontend from Vite) behave like E2E builds; they cannot run without the
//!   development server and are never distributed.

use std::ffi::OsString;
use std::sync::OnceLock;

/// Environment variables that change how the embedded browser runs (case-insensitive on Windows).
pub const ENV_OVERRIDES: &[&str] = &[
    "WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS",
    "WEBVIEW2_BROWSER_EXECUTABLE_FOLDER",
    "WEBVIEW2_USER_DATA_FOLDER",
    "WEBVIEW2_RELEASE_CHANNEL_PREFERENCE",
    "WEBVIEW2_CHANNEL_SEARCH_KIND",
    "WEBVIEW2_PIPE_FOR_SCRIPT_DEBUGGER",
    "WEBKIT_INSPECTOR_SERVER",
    "WEBKIT_INSPECTOR_HTTP_SERVER",
];

/// Browser switches that expose a debugging endpoint.
pub const DEBUG_SWITCHES: &[&str] = &[
    "--remote-debugging-port",
    "--remote-debugging-pipe",
    "--remote-debugging-address",
    "--remote-allow-origins",
    "--auto-open-devtools-for-tabs",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Production,
    DevServer,
    E2eTesting,
}

pub fn mode() -> Mode {
    if cfg!(feature = "e2e-webview-debugging") {
        Mode::E2eTesting
    } else if tauri::is_dev() {
        Mode::DevServer
    } else {
        Mode::Production
    }
}

/// Override variables present in `vars` (names as they appear, matched case-insensitively). Works on raw OS
/// strings so a non-UTF-8 value cannot hide a variable from the scrub.
pub fn overrides_in(vars: impl IntoIterator<Item = (OsString, OsString)>) -> Vec<OsString> {
    vars.into_iter()
        .map(|(k, _)| k)
        .filter(|k| {
            let name = k.to_string_lossy().to_ascii_uppercase();
            ENV_OVERRIDES.contains(&name.as_str())
        })
        .collect()
}

/// The first policy (location, arguments) whose browser arguments enable remote debugging.
pub fn debugging_policy(policies: &[(String, String)]) -> Option<String> {
    policies.iter().find_map(|(location, args)| {
        let lower = args.to_ascii_lowercase();
        DEBUG_SWITCHES.iter().any(|s| lower.contains(s)).then(|| location.clone())
    })
}

#[derive(Debug, Clone)]
pub struct Report {
    pub mode: Mode,
    /// Names of override variables removed from this process (values are never recorded).
    pub removed: Vec<String>,
}

static REPORT: OnceLock<Report> = OnceLock::new();

pub fn report() -> Option<&'static Report> {
    REPORT.get()
}

/// Harden the webview environment. Must be the first thing `main` does. `Err` carries a user-facing reason to
/// refuse to start.
#[allow(unsafe_code)]
pub fn harden() -> Result<(), String> {
    let mode = mode();
    if mode != Mode::Production {
        let _ = REPORT.set(Report { mode, removed: Vec::new() });
        return Ok(());
    }
    let found = overrides_in(std::env::vars_os());
    for name in &found {
        // SAFETY: called at the very top of `main`, before NEXUS starts any thread (no tracing, Tauri, async runtime
        // or plugin has been initialised and the Rust runtime itself spawns none), so nothing can read or write the
        // environment concurrently.
        unsafe { std::env::remove_var(name) };
    }
    if let Some(location) = debugging_policy(&registry_policies()) {
        return Err(format!(
            "NEXUS will not start because a WebView2 policy enables remote debugging for it ({location}). Remote \
             debugging would let other local programs control NEXUS. Ask your administrator to remove the setting."
        ));
    }
    let _ = REPORT.set(Report {
        mode,
        removed: found.iter().map(|n| n.to_string_lossy().into_owned()).collect(),
    });
    Ok(())
}

/// Show a native error and exit (used before the event loop exists).
pub fn refuse_to_start(message: &str) -> ! {
    eprintln!("NEXUS: {message}");
    #[cfg(windows)]
    {
        let _ = rfd::MessageDialog::new()
            .set_level(rfd::MessageLevel::Error)
            .set_title("NEXUS cannot start")
            .set_description(message)
            .set_buttons(rfd::MessageButtons::Ok)
            .show();
    }
    std::process::exit(1)
}

/// WebView2 policy values that apply to this executable: `AdditionalBrowserArguments` under the machine and user
/// policy keys, for this executable's file name and for `*`.
#[cfg(windows)]
fn registry_policies() -> Vec<(String, String)> {
    use winreg::RegKey;
    use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};
    const KEY: &str = r"SOFTWARE\Policies\Microsoft\Edge\WebView2\AdditionalBrowserArguments";
    let exe = std::env::current_exe()
        .ok()
        .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
        .unwrap_or_else(|| "nexus-desktop.exe".into());
    let mut out = Vec::new();
    for (hive, label) in [(HKEY_LOCAL_MACHINE, "HKLM"), (HKEY_CURRENT_USER, "HKCU")] {
        let Ok(key) = RegKey::predef(hive).open_subkey(KEY) else { continue };
        for name in [exe.as_str(), "*"] {
            if let Ok(value) = key.get_value::<String, _>(name) {
                out.push((format!(r"{label}\{KEY} [{name}]"), value));
            }
        }
    }
    out
}

#[cfg(not(windows))]
fn registry_policies() -> Vec<(String, String)> {
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn os(pairs: &[(&str, &str)]) -> Vec<(OsString, OsString)> {
        pairs.iter().map(|(k, v)| (OsString::from(k), OsString::from(v))).collect()
    }

    #[test]
    fn finds_every_override_in_any_case_and_nothing_else() {
        let found = overrides_in(os(&[
            ("PATH", "x"),
            ("webview2_additional_browser_arguments", "--remote-debugging-port=1"),
            ("WEBVIEW2_USER_DATA_FOLDER", "C:/shared"),
            ("WebKit_Inspector_Server", "127.0.0.1:9"),
            ("WEBVIEW2_NOT_A_THING", "x"),
        ]));
        let names: Vec<String> = found.iter().map(|n| n.to_string_lossy().into_owned()).collect();
        assert_eq!(names, ["webview2_additional_browser_arguments", "WEBVIEW2_USER_DATA_FOLDER", "WebKit_Inspector_Server"]);
    }

    #[cfg(windows)]
    #[test]
    fn non_utf8_values_cannot_hide_an_override() {
        use std::os::windows::ffi::OsStringExt;
        let bad_value = OsString::from_wide(&[0xD800]); // lone surrogate: not valid UTF-8
        let found = overrides_in(vec![(OsString::from("WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS"), bad_value)]);
        assert_eq!(found.len(), 1);
    }

    #[test]
    fn registry_policies_with_debug_switches_are_detected() {
        let policies = vec![
            ("HKLM [*]".to_string(), "--proxy-server=corp:8080".to_string()),
            ("HKCU [nexus-desktop.exe]".to_string(), "--disable-gpu --Remote-Debugging-Port=9222".to_string()),
        ];
        assert_eq!(debugging_policy(&policies).as_deref(), Some("HKCU [nexus-desktop.exe]"));
        assert_eq!(debugging_policy(&policies[..1]), None, "non-debugging admin arguments are respected");
    }

    #[test]
    fn test_builds_are_explicit() {
        // Unit tests run without the e2e feature and without the custom-protocol (embedded) frontend.
        assert_ne!(mode(), Mode::E2eTesting);
    }
}
