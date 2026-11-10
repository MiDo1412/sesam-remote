//! Tresor: Geheimnisse im Schlüsselbund des Betriebssystems
//! (Windows-Anmeldeinformationsverwaltung, macOS- bzw. iPadOS-Schlüsselbund).

#[cfg(not(target_os = "ios"))]
use keyring::{Entry, Error};
// iPadOS: keyring 4 lehnt iOS in seiner einfachen API ab; dort direkt keyring-core mit dem «protected»-Speicher
// von apple-native-keyring-store (Schlüsselbund der App, gleiche Dienst- und Kontonamen wie auf dem Mac).
#[cfg(target_os = "ios")]
use keyring_core::{Entry, Error};

#[cfg(target_os = "ios")]
fn ensure_store() -> Result<(), String> {
    static STORE: std::sync::LazyLock<Result<(), String>> = std::sync::LazyLock::new(|| {
        let store = apple_native_keyring_store::protected::Store::new().map_err(|e| e.to_string())?;
        keyring_core::set_default_store(store);
        Ok(())
    });
    STORE.clone()
}

/// Debug-Build mit eigenem Dienstnamen: läuft neben der installierten App, ohne deren Geheimnisse zu berühren.
const SERVICE: &str = if cfg!(debug_assertions) { "ch.sesam.remote.dev" } else { "ch.sesam.remote" };

#[derive(Clone, Copy, PartialEq)]
pub enum Kind {
    /// Login-Passwort (SSH/VNC) bzw. Passphrase des Schlüssels.
    Password,
    /// Eigenes VNC-Passwort (falls anders als das Kontopasswort).
    VncPassword,
    /// Eigenes RDP-Passwort (falls anders als das Kontopasswort).
    RdpPassword,
    /// Passwort für das RD-Gateway (falls eigener Gateway-Benutzer).
    GatewayPassword,
    /// Privater SSH-Schlüssel (OpenSSH oder PuTTY .ppk), ed25519 oder ECDSA (Windows erlaubt höchstens 2560 Byte pro Eintrag).
    SshKey,
}

fn entry(id: &str, kind: Kind) -> Result<Entry, String> {
    let k = match kind {
        Kind::Password => "password",
        Kind::VncPassword => "vnc-password",
        Kind::RdpPassword => "rdp-password",
        Kind::GatewayPassword => "gateway-password",
        Kind::SshKey => "ssh-key",
    };
    #[cfg(target_os = "ios")]
    ensure_store().map_err(|e| format!("Tresor nicht verfügbar: {e}"))?;
    Entry::new(SERVICE, &format!("{id}:{k}")).map_err(|e| format!("Tresor nicht verfügbar: {e}"))
}

pub fn set(id: &str, kind: Kind, secret: &str) -> Result<(), String> {
    entry(id, kind)?.set_password(secret).map_err(|e| e.to_string())
}

/// Passwörter schneller Verbindungen: nur im Arbeitsspeicher, nie im Schlüsselbund.
/// Auch der Schlüssel einer gewählten Schlüsseldatei (nur Passwort und SSH-Schlüssel).
static EPHEMERAL: std::sync::Mutex<Vec<(String, u8, String)>> = std::sync::Mutex::new(Vec::new());

pub fn set_ephemeral(id: &str, kind: Kind, secret: &str) {
    EPHEMERAL.lock().unwrap().push((id.to_string(), kind as u8, secret.to_string()));
}

pub fn get(id: &str, kind: Kind) -> Result<Option<String>, String> {
    if id.starts_with("quick-") {
        return Ok(EPHEMERAL.lock().unwrap().iter().find(|(i, k, _)| i == id && *k == kind as u8).map(|(_, _, s)| s.clone()));
    }
    match entry(id, kind)?.get_password() {
        Ok(s) => Ok(Some(s)),
        Err(Error::NoEntry) => Ok(None),
        Err(e) => Err(e.to_string()),
    }
}

pub fn delete(id: &str, kind: Kind) -> Result<(), String> {
    match entry(id, kind)?.delete_credential() {
        Ok(()) | Err(Error::NoEntry) => Ok(()),
        Err(e) => Err(e.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Schreibt in den echten Schlüsselbund: `cargo test -- --ignored`
    #[test]
    #[ignore]
    fn roundtrip_os_keychain() {
        let id = format!("selftest-{}", std::process::id());
        assert_eq!(get(&id, Kind::Password).unwrap(), None);
        set(&id, Kind::Password, "geheim äöü").unwrap();
        assert_eq!(get(&id, Kind::Password).unwrap().as_deref(), Some("geheim äöü"));
        delete(&id, Kind::Password).unwrap();
        delete(&id, Kind::Password).unwrap(); // zweimal löschen ist ok
        assert_eq!(get(&id, Kind::Password).unwrap(), None);
    }
}
