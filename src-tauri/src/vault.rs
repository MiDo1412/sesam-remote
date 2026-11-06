//! Tresor: Geheimnisse im Schlüsselbund des Betriebssystems
//! (Windows-Anmeldeinformationsverwaltung, macOS-Schlüsselbund).

use keyring::{Entry, Error};

const SERVICE: &str = "ch.sesam.remote";

#[derive(Clone, Copy)]
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
    Entry::new(SERVICE, &format!("{id}:{k}")).map_err(|e| format!("Tresor nicht verfügbar: {e}"))
}

pub fn set(id: &str, kind: Kind, secret: &str) -> Result<(), String> {
    entry(id, kind)?.set_password(secret).map_err(|e| e.to_string())
}

/// Passwörter schneller Verbindungen: nur im Arbeitsspeicher, nie im Schlüsselbund.
static EPHEMERAL: std::sync::Mutex<Vec<(String, String)>> = std::sync::Mutex::new(Vec::new());

pub fn set_ephemeral(id: &str, secret: &str) {
    EPHEMERAL.lock().unwrap().push((id.to_string(), secret.to_string()));
}

pub fn get(id: &str, kind: Kind) -> Result<Option<String>, String> {
    if id.starts_with("quick-") {
        return Ok(match kind {
            Kind::Password => EPHEMERAL.lock().unwrap().iter().find(|(i, _)| i == id).map(|(_, s)| s.clone()),
            _ => None,
        });
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
