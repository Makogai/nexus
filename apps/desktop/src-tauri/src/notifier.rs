//! Native desktop notifications (ADR-0025). Notifications are produced by every NEXUS process (hooks, MCP, CLI,
//! this app) into SQLite; only the desktop shows them. A poller delivers pending ones through a [`Notifier`]:
//!
//! * Windows — WinRT toasts (`tauri-winrt-notification`); a click focuses NEXUS and opens the target. Development
//!   builds (under `target\debug|release`) show as "Windows PowerShell", like Tauri's notification plugin, because an
//!   unpackaged executable has no registered AppUserModelID; installed builds use the bundle identifier.
//! * macOS / Linux — Tauri's notification plugin (display only: the plugin reports no clicks on desktop).
//! * E2E builds (`e2e-webview-debugging` + `NEXUS_E2E_NOTIFIER=record`) — a recording adapter, so tests can check
//!   what would have been shown and simulate a click without touching the OS.
//!
//! The OS decides in the end: Focus Assist / Do Not Disturb or a disabled app setting suppresses toasts silently;
//! where the OS reports that (Windows notifier setting), [`Notifier::availability`] says so.

use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use nexus_core::notify::{self, Focus, Notification};

use crate::state::Ctx;

pub const ACTIVATED_EVENT: &str = "nexus://notification-activated";
pub const CHANGED_EVENT: &str = "nexus://notifications";
const POLL: Duration = Duration::from_millis(750);

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Availability {
    pub backend: &'static str,
    pub available: bool,
    /// Why the OS will not show notifications (e.g. turned off in Windows settings), if known.
    pub reason: Option<String>,
    /// Clicking a notification opens its target in NEXUS.
    pub click_to_open: bool,
}

pub trait Notifier: Send + Sync {
    fn availability(&self, app: &AppHandle) -> Availability;
    fn show(&self, app: &AppHandle, n: &Notification, sound: bool) -> Result<(), String>;
    /// Recording adapter only: what was shown.
    fn recorded(&self) -> Option<Vec<Notification>> {
        None
    }
}

/// Bring NEXUS to the front and tell the UI to open the notification's target.
pub fn activate(app: &AppHandle, id: &str) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
    if let Some(ctx) = app.try_state::<Ctx>() {
        let _ = notify::mark_read(&ctx.db(), &[id.to_string()]);
    }
    let _ = app.emit(ACTIVATED_EVENT, id.to_string());
}

#[cfg(windows)]
mod windows_toast {
    use super::*;
    use tauri_winrt_notification::{Sound, Toast};
    use windows::UI::Notifications::{NotificationSetting, ToastNotificationManager};
    use windows::core::HSTRING;

    pub struct WindowsToast;

    fn app_id(app: &AppHandle) -> String {
        let dev = tauri::utils::platform::current_exe()
            .ok()
            .and_then(|e| e.parent().map(|p| p.to_path_buf()))
            .map(|d| {
                let d = d.to_string_lossy().to_lowercase();
                d.ends_with("\\target\\debug") || d.ends_with("\\target\\release")
            })
            .unwrap_or(true);
        if dev { Toast::POWERSHELL_APP_ID.to_string() } else { app.config().identifier.clone() }
    }

    impl Notifier for WindowsToast {
        fn availability(&self, app: &AppHandle) -> Availability {
            let setting = ToastNotificationManager::CreateToastNotifierWithId(&HSTRING::from(app_id(app))).and_then(|n| n.Setting());
            let reason = match setting {
                Ok(NotificationSetting::Enabled) => None,
                Ok(NotificationSetting::DisabledForApplication) => Some("Notifications for NEXUS are turned off in Windows settings".to_string()),
                Ok(NotificationSetting::DisabledForUser) => Some("Notifications are turned off in Windows settings".to_string()),
                Ok(NotificationSetting::DisabledByGroupPolicy) => Some("Notifications are disabled by an organisation policy".to_string()),
                Ok(_) => Some("Windows does not allow notifications for this app".to_string()),
                Err(e) => Some(format!("Windows notification service unavailable ({})", e.code())),
            };
            Availability { backend: "windows", available: reason.is_none(), reason, click_to_open: true }
        }

        fn show(&self, app: &AppHandle, n: &Notification, sound: bool) -> Result<(), String> {
            let handle = app.clone();
            let id = n.id.clone();
            Toast::new(&app_id(app))
                .title(&n.title)
                .text1(&n.body)
                .sound(if sound { Some(Sound::Default) } else { None })
                .on_activated(move |_action| {
                    activate(&handle, &id);
                    Ok(())
                })
                .show()
                .map_err(|e| e.to_string())
        }
    }
}

#[cfg(not(windows))]
mod plugin {
    use super::*;
    use tauri_plugin_notification::NotificationExt;

    pub struct PluginNotifier;

    impl Notifier for PluginNotifier {
        fn availability(&self, app: &AppHandle) -> Availability {
            let granted = matches!(app.notification().permission_state(), Ok(tauri_plugin_notification::PermissionState::Granted));
            Availability {
                backend: "tauri-plugin-notification",
                available: granted,
                reason: (!granted).then(|| "The system has not granted notification permission".to_string()),
                click_to_open: false,
            }
        }

        fn show(&self, app: &AppHandle, n: &Notification, sound: bool) -> Result<(), String> {
            let mut b = app.notification().builder().title(&n.title).body(&n.body);
            if sound {
                b = b.sound("default");
            }
            b.show().map_err(|e| e.to_string())
        }
    }
}

#[cfg(feature = "e2e-webview-debugging")]
mod recording {
    use super::*;
    use std::sync::Mutex;

    #[derive(Default)]
    pub struct Recording(pub Mutex<Vec<Notification>>);

    impl Notifier for Recording {
        fn availability(&self, _app: &AppHandle) -> Availability {
            Availability { backend: "recording", available: true, reason: None, click_to_open: true }
        }
        fn show(&self, _app: &AppHandle, n: &Notification, _sound: bool) -> Result<(), String> {
            self.0.lock().unwrap_or_else(|p| p.into_inner()).push(n.clone());
            Ok(())
        }
        fn recorded(&self) -> Option<Vec<Notification>> {
            Some(self.0.lock().unwrap_or_else(|p| p.into_inner()).clone())
        }
    }
}

pub fn select() -> Box<dyn Notifier> {
    #[cfg(feature = "e2e-webview-debugging")]
    if std::env::var("NEXUS_E2E_NOTIFIER").as_deref() == Ok("record") {
        return Box::new(recording::Recording::default());
    }
    #[cfg(windows)]
    {
        Box::new(windows_toast::WindowsToast)
    }
    #[cfg(not(windows))]
    {
        Box::new(plugin::PluginNotifier)
    }
}

pub fn focus(app: &AppHandle, ctx: &Ctx) -> Focus {
    let window_focused = app
        .get_webview_window("main")
        .map(|w| w.is_focused().unwrap_or(false) && !w.is_minimized().unwrap_or(false) && w.is_visible().unwrap_or(true))
        .unwrap_or(false);
    let view = ctx.attention.view();
    let utc_minutes = nexus_core::now_ms() / 60_000;
    Focus { window_focused, visible_session: view.visible_session, local_minutes: (utc_minutes - view.utc_offset_minutes).rem_euclid(24 * 60) }
}

/// Deliver pending notifications and tell the UI when the notification set changed (other processes write too).
pub fn spawn_poller(app: AppHandle, ctx: Ctx) {
    let _ = std::thread::Builder::new().name("notifications".into()).spawn(move || {
        let mut last_fingerprint = None;
        let mut last_cleanup = 0i64;
        loop {
            std::thread::sleep(POLL);
            let focus = focus(&app, &ctx);
            let (deliver, fingerprint, sound) = {
                let db = ctx.db();
                let prefs = notify::prefs(&db).unwrap_or_default();
                let mut deliver = Vec::new();
                for n in notify::pending_desktop(&db, 20).unwrap_or_default() {
                    match notify::desktop_decision(&prefs, &n, &focus) {
                        Ok(()) => {
                            let _ = notify::mark_desktop(&db, &n.id, true, None);
                            deliver.push(n);
                        }
                        Err(reason) => {
                            let _ = notify::mark_desktop(&db, &n.id, false, Some(&reason));
                        }
                    }
                }
                let now = nexus_core::now_ms();
                if now - last_cleanup > 3_600_000 {
                    last_cleanup = now;
                    let _ = notify::cleanup(&db, notify::RETENTION_DAYS, false);
                }
                (deliver, notify::fingerprint(&db).ok(), prefs.sound)
            };
            for n in deliver {
                if let Err(e) = ctx.notifier.show(&app, &n, sound) {
                    tracing::warn!(error = %e, "desktop notification failed");
                }
            }
            if fingerprint != last_fingerprint {
                last_fingerprint = fingerprint;
                let _ = app.emit(CHANGED_EVENT, ());
            }
        }
    });
}
