//! Tresor: Geheimnisse im Schlüsselbund des Betriebssystems
//! (Windows-Anmeldeinformationsverwaltung, macOS-Schlüsselbund).

use keyring::{Entry, Error};

const SERVICE: &str = "ch.sesam.remote";

#[derive(Clone, Copy)]
pub enum Kind {
    /// Login-Passwort (SSH/VNC) bzw. Passphrase des Schlüssels.
    Password,
    /// Privater SSH-Schlüssel im OpenSSH-Format.
    // shortcut: Windows erlaubt höchstens 2560 Byte pro Eintrag; reicht für ed25519/ECDSA, nicht für RSA-4096. Bei Bedarf Schlüssel aufteilen oder verschlüsselt im Datenordner ablegen.
    SshKey,
}

fn entry(id: &str, kind: Kind) -> Result<Entry, String> {
    let k = match kind {
        Kind::Password => "password",
        Kind::SshKey => "ssh-key",
    };
    Entry::new(SERVICE, &format!("{id}:{k}")).map_err(|e| format!("Tresor nicht verfügbar: {e}"))
}

pub fn set(id: &str, kind: Kind, secret: &str) -> Result<(), String> {
    entry(id, kind)?.set_password(secret).map_err(|e| e.to_string())
}

pub fn get(id: &str, kind: Kind) -> Result<Option<String>, String> {
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
