//! Native desktop notifications (ADR-0025). Notifications are produced by every NEXUS process (hooks, MCP, CLI,
//! this app) into SQLite; only the desktop shows them. A poller delivers pending ones through a [`Notifier`]:
//!
//! * Windows — WinRT toasts built here (no third-party wrapper). NEXUS registers **its own sender identity**
//!   (AppUserModelID under `HKCU\Software\Classes\AppUserModelId`, the documented route for unpackaged apps):
//!   `dev.nexus.desktop` "NEXUS" for installed builds, `dev.nexus.desktop.dev` "NEXUS (development)" for builds under
//!   `target\`. Toasts carry the notification id as tag, so handled notifications are withdrawn from the Action
//!   Center and stale ones are cleared at start. A click focuses NEXUS and opens the target — in-process while the
//!   toast object lives, and in installed builds also through the `nexus://notification/<id>` protocol (works from
//!   the Action Center after a restart; routed by the single-instance plugin).
//! * macOS / Linux — Tauri's notification plugin (display only: the plugin reports no clicks on desktop).
//! * E2E builds (`e2e-webview-debugging` + `NEXUS_E2E_NOTIFIER=record`) — a recording adapter.
//!
//! Content follows the user's lock-screen policy ([`notify::desktop_text`]): by default no project, session, tool or
//! file names while the workstation is locked (best-effort detection), or never. The OS has the last word: Focus
//! Assist / Do Not Disturb or a disabled app setting suppresses toasts; where Windows reports that,
//! [`Notifier::availability`] says so.

use std::collections::HashSet;
use std::sync::Mutex;
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use nexus_core::notify::{self, Focus, Notification};

use crate::state::Ctx;

pub const ACTIVATED_EVENT: &str = "nexus://notification-activated";
pub const CHANGED_EVENT: &str = "nexus://notifications";
pub const PROTOCOL: &str = "nexus";
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
    /// The sender name the OS shows (Windows: the registered AppUserModelID's display name).
    pub sender: Option<String>,
    /// Clicks also work from the notification center after NEXUS restarted (protocol activation).
    pub click_after_restart: bool,
}

pub trait Notifier: Send + Sync {
    fn availability(&self, app: &AppHandle) -> Availability;
    /// Show a notification with the text the content policy allows.
    fn show(&self, app: &AppHandle, n: &Notification, title: &str, body: &str, sound: bool) -> Result<(), String>;
    /// Ids of notifications currently shown by this process.
    fn shown(&self) -> Vec<String> {
        Vec::new()
    }
    /// Remove these notifications from the OS notification center (handled in NEXUS).
    fn withdraw(&self, _ids: &[String]) {}
    /// Startup: register the sender and clear stale notifications of an earlier run.
    fn prepare(&self, _app: &AppHandle) {}
    /// Recording adapter only: what was shown (with the shown text) and what was withdrawn.
    fn recorded(&self) -> Option<(Vec<Notification>, Vec<String>)> {
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

/// `nexus://notification/<uuid>` → the notification id (anything else is ignored).
pub fn parse_activation(arg: &str) -> Option<String> {
    let rest = arg.trim().trim_matches('"');
    let lower = rest.to_ascii_lowercase();
    let id = lower.strip_prefix("nexus://notification/")?.trim_end_matches('/');
    uuid::Uuid::parse_str(id).ok().map(|u| u.to_string())
}

fn dev_build() -> bool {
    tauri::utils::platform::current_exe()
        .ok()
        .and_then(|e| e.parent().map(|p| p.to_string_lossy().to_lowercase()))
        .map(|d| d.ends_with("\\target\\debug") || d.ends_with("\\target\\release") || d.ends_with("/target/debug") || d.ends_with("/target/release"))
        .unwrap_or(true)
}

/// Best-effort: is the workstation locked? (Windows: the lock screen's LogonUI process runs.) Cached for 2 s.
fn workstation_locked() -> bool {
    #[cfg(windows)]
    {
        static CACHE: Mutex<Option<(std::time::Instant, bool)>> = Mutex::new(None);
        let mut c = CACHE.lock().unwrap_or_else(|p| p.into_inner());
        if let Some((at, v)) = *c
            && at.elapsed() < Duration::from_secs(2)
        {
            return v;
        }
        let mut cmd = nexus_core::proc::command("tasklist");
        cmd.args(["/FI", "IMAGENAME eq LogonUI.exe", "/NH", "/FO", "CSV"]);
        let v = nexus_core::proc::run(cmd, Duration::from_secs(3)).map(|o| o.stdout_str().to_ascii_lowercase().contains("logonui.exe")).unwrap_or(false);
        *c = Some((std::time::Instant::now(), v));
        v
    }
    #[cfg(not(windows))]
    {
        false
    }
}

#[cfg(windows)]
pub mod windows_toast {
    use super::*;
    use std::collections::HashMap;
    use std::path::{Path, PathBuf};
    use std::sync::OnceLock;
    use windows::Data::Xml::Dom::XmlDocument;
    use windows::Foundation::TypedEventHandler;
    use windows::UI::Notifications::{NotificationSetting, ToastNotification, ToastNotificationManager};
    use windows::core::HSTRING;
    use winreg::RegKey;
    use winreg::enums::HKEY_CURRENT_USER;

    pub const GROUP: &str = "nexus";

    /// The sender identity of this build.
    #[derive(Debug, Clone)]
    pub struct Identity {
        pub aumid: String,
        pub display: String,
        /// Installed builds register `nexus://` so a click works after a restart.
        pub protocol: bool,
    }

    pub fn identity(identifier: &str) -> Identity {
        if dev_build() {
            Identity { aumid: format!("{identifier}.dev"), display: "NEXUS (development)".into(), protocol: false }
        } else {
            Identity { aumid: identifier.to_string(), display: "NEXUS".into(), protocol: true }
        }
    }

    /// Register (or refresh) the AppUserModelID so Windows shows NEXUS as the sender. NEXUS-owned key only.
    pub fn register(aumid: &str, display: &str, icon: Option<&Path>) -> Result<(), String> {
        let (key, _) = RegKey::predef(HKEY_CURRENT_USER).create_subkey(format!(r"Software\Classes\AppUserModelId\{aumid}")).map_err(|e| e.to_string())?;
        key.set_value("DisplayName", &display.to_string()).map_err(|e| e.to_string())?;
        if let Some(i) = icon {
            key.set_value("IconUri", &i.to_string_lossy().to_string()).map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    pub fn unregister(aumid: &str) {
        let _ = RegKey::predef(HKEY_CURRENT_USER).delete_subkey_all(format!(r"Software\Classes\AppUserModelId\{aumid}"));
    }

    /// `nexus://` → this executable (installed builds). The handler only focuses NEXUS and opens a notification.
    pub fn register_protocol(exe: &Path) -> Result<(), String> {
        let (key, _) = RegKey::predef(HKEY_CURRENT_USER).create_subkey(format!(r"Software\Classes\{PROTOCOL}")).map_err(|e| e.to_string())?;
        key.set_value("", &"URL:NEXUS notification".to_string()).map_err(|e| e.to_string())?;
        key.set_value("URL Protocol", &String::new()).map_err(|e| e.to_string())?;
        let (cmd, _) = key.create_subkey(r"shell\open\command").map_err(|e| e.to_string())?;
        cmd.set_value("", &format!("\"{}\" \"%1\"", exe.display())).map_err(|e| e.to_string())?;
        Ok(())
    }

    fn xml_escape(s: &str) -> String {
        s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;").replace('\'', "&apos;")
    }

    pub fn toast_xml(id: &str, protocol: bool, title: &str, body: &str, sound: bool) -> String {
        let (launch, activation) = if protocol { (format!("{PROTOCOL}://notification/{id}"), "protocol") } else { (id.to_string(), "foreground") };
        format!(
            r#"<toast launch="{}" activationType="{activation}"><visual><binding template="ToastGeneric"><text>{}</text><text>{}</text></binding></visual>{}</toast>"#,
            xml_escape(&launch),
            xml_escape(title),
            xml_escape(body),
            if sound { "" } else { r#"<audio silent="true"/>"# }
        )
    }

    /// Show a toast (tag = notification id, group `nexus`). `suppress_popup` sends it straight to the Action Center.
    pub fn show_raw(aumid: &str, id: &str, xml: &str, suppress_popup: bool, on_click: Option<Box<dyn Fn() + Send + 'static>>) -> Result<ToastNotification, String> {
        let e = |e: windows::core::Error| e.to_string();
        let doc = XmlDocument::new().map_err(e)?;
        doc.LoadXml(&HSTRING::from(xml)).map_err(e)?;
        let toast = ToastNotification::CreateToastNotification(&doc).map_err(e)?;
        toast.SetTag(&HSTRING::from(id)).map_err(e)?;
        toast.SetGroup(&HSTRING::from(GROUP)).map_err(e)?;
        toast.SetSuppressPopup(suppress_popup).map_err(e)?;
        if let Some(f) = on_click {
            toast
                .Activated(&TypedEventHandler::new(move |_, _| {
                    f();
                    Ok(())
                }))
                .map_err(e)?;
        }
        ToastNotificationManager::CreateToastNotifierWithId(&HSTRING::from(aumid)).and_then(|n| n.Show(&toast)).map_err(e)?;
        Ok(toast)
    }

    /// Tags of this sender's notifications in the Action Center.
    pub fn history_tags(aumid: &str) -> Result<Vec<String>, String> {
        let list = ToastNotificationManager::History().and_then(|h| h.GetHistoryWithId(&HSTRING::from(aumid))).map_err(|e| e.to_string())?;
        let n = list.Size().map_err(|e| e.to_string())?;
        Ok((0..n).filter_map(|i| list.GetAt(i).ok()).filter_map(|t| t.Tag().ok()).map(|t| t.to_string()).collect())
    }

    pub fn remove(aumid: &str, id: &str) {
        let _ = ToastNotificationManager::History().and_then(|h| h.RemoveGroupedTagWithId(&HSTRING::from(id), &HSTRING::from(GROUP), &HSTRING::from(aumid)));
    }

    pub fn clear(aumid: &str) {
        let _ = ToastNotificationManager::History().and_then(|h| h.ClearWithId(&HSTRING::from(aumid)));
    }

    pub fn setting(aumid: &str) -> Result<NotificationSetting, String> {
        ToastNotificationManager::CreateToastNotifierWithId(&HSTRING::from(aumid)).and_then(|n| n.Setting()).map_err(|e| e.to_string())
    }

    #[derive(Default)]
    pub struct WindowsToast {
        ready: OnceLock<Result<Identity, String>>,
        /// Shown toasts (kept alive so in-process clicks keep working), by notification id.
        shown: Mutex<HashMap<String, ToastNotification>>,
    }

    impl WindowsToast {
        pub fn new() -> Self {
            WindowsToast { ready: OnceLock::new(), shown: Mutex::new(HashMap::new()) }
        }

        fn init(&self, app: &AppHandle) -> Result<&Identity, String> {
            self.ready
                .get_or_init(|| {
                    let id = identity(&app.config().identifier);
                    let icon: Option<PathBuf> = app.try_state::<Ctx>().map(|c| c.data_dir.join("notification-icon.png"));
                    if let Some(i) = &icon {
                        let _ = std::fs::write(i, include_bytes!("../icons/128x128.png"));
                    }
                    register(&id.aumid, &id.display, icon.as_deref())?;
                    if id.protocol
                        && let Ok(exe) = tauri::utils::platform::current_exe()
                        && let Err(e) = register_protocol(&exe)
                    {
                        tracing::warn!(error = %e, "nexus:// protocol not registered");
                    }
                    // Notifications of an earlier run point at sessions that no longer exist here.
                    clear(&id.aumid);
                    Ok(id)
                })
                .as_ref()
                .map_err(|e| e.clone())
        }
    }

    impl Notifier for WindowsToast {
        fn prepare(&self, app: &AppHandle) {
            // Installed builds register at start; development builds on their first notification.
            if !dev_build()
                && let Err(e) = self.init(app)
            {
                tracing::warn!(error = %e, "notification sender not registered");
            }
        }

        fn availability(&self, app: &AppHandle) -> Availability {
            let id = identity(&app.config().identifier);
            let reason = match setting(&id.aumid) {
                Ok(NotificationSetting::Enabled) => None,
                Ok(NotificationSetting::DisabledForApplication) => Some("Notifications for NEXUS are turned off in Windows settings".to_string()),
                Ok(NotificationSetting::DisabledForUser) => Some("Notifications are turned off in Windows settings".to_string()),
                Ok(NotificationSetting::DisabledByGroupPolicy) => Some("Notifications are disabled by an organisation policy".to_string()),
                Ok(_) => Some("Windows does not allow notifications for this app".to_string()),
                // Verified on Windows 11: a registered sender has no notifier setting until its first toast
                // ("Element not found") — not a sign that notifications are off.
                Err(_) => None,
            };
            Availability { backend: "windows", available: reason.is_none(), reason, click_to_open: true, sender: Some(id.display), click_after_restart: id.protocol }
        }

        fn show(&self, app: &AppHandle, n: &Notification, title: &str, body: &str, sound: bool) -> Result<(), String> {
            let id = self.init(app)?.clone();
            let (h, nid) = (app.clone(), n.id.clone());
            let xml = toast_xml(&n.id, id.protocol, title, body, sound);
            let toast = show_raw(&id.aumid, &n.id, &xml, false, Some(Box::new(move || activate(&h, &nid))))?;
            let mut shown = self.shown.lock().unwrap_or_else(|p| p.into_inner());
            if shown.len() > 200 {
                shown.clear();
            }
            shown.insert(n.id.clone(), toast);
            Ok(())
        }

        fn shown(&self) -> Vec<String> {
            self.shown.lock().unwrap_or_else(|p| p.into_inner()).keys().cloned().collect()
        }

        fn withdraw(&self, ids: &[String]) {
            let Some(Ok(id)) = self.ready.get() else { return };
            let mut shown = self.shown.lock().unwrap_or_else(|p| p.into_inner());
            for i in ids {
                remove(&id.aumid, i);
                shown.remove(i);
            }
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
                sender: Some("NEXUS".into()),
                click_after_restart: false,
            }
        }

        fn show(&self, app: &AppHandle, _n: &Notification, title: &str, body: &str, sound: bool) -> Result<(), String> {
            let mut b = app.notification().builder().title(title).body(body);
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

    #[derive(Default)]
    pub struct Recording {
        shown: Mutex<Vec<Notification>>,
        withdrawn: Mutex<Vec<String>>,
        live: Mutex<HashSet<String>>,
    }

    impl Notifier for Recording {
        fn availability(&self, _app: &AppHandle) -> Availability {
            Availability { backend: "recording", available: true, reason: None, click_to_open: true, sender: Some("NEXUS (recording)".into()), click_after_restart: true }
        }
        fn show(&self, _app: &AppHandle, n: &Notification, title: &str, body: &str, _sound: bool) -> Result<(), String> {
            let mut shown = n.clone();
            shown.title = title.to_string();
            shown.body = body.to_string();
            self.shown.lock().unwrap_or_else(|p| p.into_inner()).push(shown);
            self.live.lock().unwrap_or_else(|p| p.into_inner()).insert(n.id.clone());
            Ok(())
        }
        fn shown(&self) -> Vec<String> {
            self.live.lock().unwrap_or_else(|p| p.into_inner()).iter().cloned().collect()
        }
        fn withdraw(&self, ids: &[String]) {
            let mut live = self.live.lock().unwrap_or_else(|p| p.into_inner());
            for i in ids {
                live.remove(i);
            }
            self.withdrawn.lock().unwrap_or_else(|p| p.into_inner()).extend(ids.iter().cloned());
        }
        fn recorded(&self) -> Option<(Vec<Notification>, Vec<String>)> {
            Some((self.shown.lock().unwrap_or_else(|p| p.into_inner()).clone(), self.withdrawn.lock().unwrap_or_else(|p| p.into_inner()).clone()))
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
        Box::new(windows_toast::WindowsToast::new())
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

/// Shown notifications that were handled in NEXUS (read, dismissed, resolved) — to withdraw from the OS.
fn handled(db: &nexus_core::db::Connection, ids: &[String]) -> Vec<String> {
    let set: HashSet<&String> = ids.iter().collect();
    notify::list(db, &notify::Filter { include_dismissed: true, limit: Some(1000), ..Default::default() })
        .unwrap_or_default()
        .into_iter()
        .filter(|n| set.contains(&n.id) && (n.read_at.is_some() || n.dismissed_at.is_some() || n.resolved_at.is_some()))
        .map(|n| n.id)
        .collect()
}

/// Deliver pending notifications and tell the UI when the notification set changed (other processes write too).
pub fn spawn_poller(app: AppHandle, ctx: Ctx) {
    ctx.notifier.prepare(&app);
    let _ = std::thread::Builder::new().name("notifications".into()).spawn(move || {
        let mut last_fingerprint = None;
        let mut last_cleanup = 0i64;
        loop {
            std::thread::sleep(POLL);
            let focus = focus(&app, &ctx);
            let shown = ctx.notifier.shown();
            let (deliver, fingerprint, prefs, withdraw) = {
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
                let withdraw = if shown.is_empty() { vec![] } else { handled(&db, &shown) };
                (deliver, notify::fingerprint(&db).ok(), prefs, withdraw)
            };
            if !withdraw.is_empty() {
                ctx.notifier.withdraw(&withdraw);
            }
            let locked = !deliver.is_empty() && prefs.lock_screen == notify::LockScreen::HideDetails && workstation_locked();
            for n in deliver {
                let (title, body) = notify::desktop_text(&prefs, &n, locked);
                if let Err(e) = ctx.notifier.show(&app, &n, &title, &body, prefs.sound) {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protocol_activation_accepts_only_notification_uuids() {
        let id = "0f8b1c2e-6a4d-4e2a-9d4b-2f1c3b5a7e90";
        assert_eq!(parse_activation(&format!("nexus://notification/{id}")).as_deref(), Some(id));
        assert_eq!(parse_activation(&format!("\"NEXUS://notification/{}/\"", id.to_uppercase())).as_deref(), Some(id));
        for bad in ["nexus://notification/../../etc", "nexus://run/x", "https://example.com", "nexus://notification/", "--flag"] {
            assert_eq!(parse_activation(bad), None, "{bad}");
        }
    }

    #[cfg(windows)]
    #[test]
    fn toast_xml_escapes_text_and_uses_the_protocol_only_when_registered() {
        let x = windows_toast::toast_xml("id-1", true, "A <b> & \"c\"", "body", false);
        assert!(x.contains(r#"launch="nexus://notification/id-1" activationType="protocol""#) && x.contains("A &lt;b&gt; &amp; &quot;c&quot;") && x.contains(r#"<audio silent="true"/>"#));
        assert!(windows_toast::toast_xml("id-1", false, "t", "b", true).contains(r#"launch="id-1" activationType="foreground""#));
    }
}
