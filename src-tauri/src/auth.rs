//! Bestätigung durch das System vor heiklen Aktionen (macOS, iPadOS: LocalAuthentication).

/// Touch ID bzw. Face ID (oder das Passwort/der Code des Geräts) bestätigen lassen; `reason` steht im Systemdialog.
/// Blockiert bis zur Antwort, darum nur ausserhalb des Hauptthreads aufrufen.
pub fn authenticate(reason: &str) -> Result<(), String> {
    use objc2::runtime::{AnyObject, Bool};
    use objc2::{class, msg_send, rc::Retained};
    #[link(name = "LocalAuthentication", kind = "framework")]
    unsafe extern "C" {}
    let (tx, rx) = std::sync::mpsc::channel::<bool>();
    let block = block2::RcBlock::new(move |ok: Bool, _err: *mut AnyObject| {
        let _ = tx.send(ok.as_bool());
    });
    // LAPolicyDeviceOwnerAuthentication = 2: Touch ID, Face ID, Watch oder Passwort/Code des Geräts.
    unsafe {
        let ctx: Retained<AnyObject> = msg_send![class!(LAContext), new];
        let reason = objc2_foundation::NSString::from_str(reason);
        let _: () = msg_send![&*ctx, evaluatePolicy: 2isize, localizedReason: &*reason, reply: &*block];
    }
    match rx.recv_timeout(std::time::Duration::from_secs(120)) {
        Ok(true) => Ok(()),
        _ => Err("Nicht bestätigt".into()),
    }
}

