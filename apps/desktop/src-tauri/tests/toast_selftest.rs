//! Real Windows notification self-test (opt-in: `cargo test -p nexus-desktop --test toast_selftest -- --ignored`).
//! Registers a temporary, NEXUS-owned sender (`dev.nexus.desktop.selftest`), shows one toast **without a popup**
//! (straight to the Action Center), checks Windows filed it under that sender, withdraws it and removes the
//! registration again — also when an assertion fails. Nothing else on the machine is changed.

#[cfg(windows)]
#[test]
#[ignore = "touches the real Windows notification platform; run explicitly"]
fn nexus_is_the_registered_sender_and_handled_toasts_are_withdrawn() {
    use nexus_desktop_lib::notifier::windows_toast as t;

    const AUMID: &str = "dev.nexus.desktop.selftest";
    struct Cleanup;
    impl Drop for Cleanup {
        fn drop(&mut self) {
            t::clear(AUMID);
            t::unregister(AUMID);
        }
    }
    let _cleanup = Cleanup;

    t::unregister(AUMID);
    t::register(AUMID, "NEXUS (self-test)", None).expect("register the sender");
    let tag = "0f8b1c2e-6a4d-4e2a-9d4b-2f1c3b5a7e90";
    let xml = t::toast_xml(tag, false, "NEXUS self-test", "Generic text only", false);
    let _toast = t::show_raw(AUMID, tag, &xml, true, None).expect("Windows accepts a toast from NEXUS's own sender");
    std::thread::sleep(std::time::Duration::from_millis(500));
    let tags = t::history_tags(AUMID).expect("history");
    assert!(tags.iter().any(|x| x == tag), "the toast is filed under NEXUS's sender: {tags:?}");
    // Observed on Windows 11: before its first toast a registry-registered sender has no notifier setting ("Element
    // not found"); after it, Windows reports Enabled — NEXUS therefore never reads that error as "notifications off".
    assert_eq!(t::setting(AUMID), Ok(windows::UI::Notifications::NotificationSetting::Enabled));

    t::remove(AUMID, tag);
    std::thread::sleep(std::time::Duration::from_millis(300));
    assert!(!t::history_tags(AUMID).unwrap_or_default().iter().any(|x| x == tag), "withdrawn from the Action Center");
}
