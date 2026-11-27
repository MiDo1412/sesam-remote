//! Bestätigung durch das System vor heiklen Aktionen (macOS, iPadOS: LocalAuthentication; Windows: Windows Hello).

/// Touch ID bzw. Face ID (oder das Passwort/der Code des Geräts) bestätigen lassen; `reason` steht im Systemdialog.
/// Blockiert bis zur Antwort, darum nur ausserhalb des Hauptthreads aufrufen.
#[cfg(any(target_os = "macos", target_os = "ios"))]
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


/// Windows Hello (Gesicht, Fingerabdruck oder PIN), der Dialog gehört zum Fenster `hwnd`. `Ok(false)`, wenn Hello auf
/// diesem Gerät nicht eingerichtet ist. Blockiert bis zur Antwort, darum nur ausserhalb des Hauptthreads aufrufen.
#[cfg(windows)]
pub fn hello(hwnd: isize, reason: &str) -> Result<bool, String> {
    use windows::Security::Credentials::UI::{UserConsentVerificationResult, UserConsentVerifier, UserConsentVerifierAvailability};
    use windows::Win32::System::WinRT::IUserConsentVerifierInterop;
    let e = |e: windows::core::Error| format!("Windows Hello: {e}");
    let available = UserConsentVerifier::CheckAvailabilityAsync().and_then(|o| o.join()).map_err(e)?;
    if available != UserConsentVerifierAvailability::Available {
        return Ok(false);
    }
    // Desktop-Apps brauchen die Interop-Variante mit Fenster, sonst erscheint der Dialog hinter der App.
    let interop = windows::core::factory::<UserConsentVerifier, IUserConsentVerifierInterop>().map_err(e)?;
    let op: windows_future::IAsyncOperation<UserConsentVerificationResult> =
        unsafe { interop.RequestVerificationForWindowAsync(windows::Win32::Foundation::HWND(hwnd as *mut _), &windows::core::HSTRING::from(reason)) }.map_err(e)?;
    match op.join().map_err(e)? {
        UserConsentVerificationResult::Verified => Ok(true),
        _ => Err("Nicht bestätigt".into()),
    }
}
