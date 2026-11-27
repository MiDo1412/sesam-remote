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
    /// Privater SSH-Schlüssel (OpenSSH, PEM oder PuTTY .ppk); lange Schlüssel (RSA) in Teilen, siehe `PART`.
    SshKey,
}

/// Windows erlaubt höchstens 2560 Byte (1280 UTF-16-Zeichen) pro Eintrag. Längere Geheimnisse (RSA-Schlüssel)
/// liegen in Teilen: der Haupteintrag hält nur `PARTS` und die Anzahl, die Teile heissen `…#1`, `…#2` usw.
const PART: usize = 1200;
const PARTS: &str = "sesam-parts:";

fn entry(id: &str, kind: Kind) -> Result<Entry, String> {
    entry_part(id, kind, 0)
}

fn entry_part(id: &str, kind: Kind, part: usize) -> Result<Entry, String> {
    let k = match kind {
        Kind::Password => "password",
        Kind::VncPassword => "vnc-password",
        Kind::RdpPassword => "rdp-password",
        Kind::GatewayPassword => "gateway-password",
        Kind::SshKey => "ssh-key",
    };
    #[cfg(target_os = "ios")]
    ensure_store().map_err(|e| format!("Tresor nicht verfügbar: {e}"))?;
    let user = if part == 0 { format!("{id}:{k}") } else { format!("{id}:{k}#{part}") };
    Entry::new(SERVICE, &user).map_err(|e| format!("Tresor nicht verfügbar: {e}"))
}

/// Erst alles Neue schreiben, dann den Haupteintrag umstellen, zuletzt überzählige alte Teile löschen: scheitert
/// ein Schreiben mittendrin, bleibt das alte Geheimnis lesbar (ausser dort überschriebene Teile).
pub fn set(id: &str, kind: Kind, secret: &str) -> Result<(), String> {
    let old = part_count(id, kind)?;
    let new = if secret.encode_utf16().count() <= PART {
        entry(id, kind)?.set_password(secret).map_err(|e| e.to_string())?;
        0
    } else {
        // Halbe Länge in Zeichen: auch Zeichen aus zwei UTF-16-Einheiten passen sicher.
        let chars: Vec<char> = secret.chars().collect();
        let parts: Vec<String> = chars.chunks(PART / 2).map(|c| c.iter().collect()).collect();
        for (i, p) in parts.iter().enumerate() {
            entry_part(id, kind, i + 1)?.set_password(p).map_err(|e| e.to_string())?;
        }
        entry(id, kind)?.set_password(&format!("{PARTS}{}", parts.len())).map_err(|e| e.to_string())?;
        parts.len()
    };
    delete_part_range(id, kind, new + 1, old)
}

fn get_raw(e: Entry) -> Result<Option<String>, String> {
    match e.get_password() {
        Ok(s) => Ok(Some(s)),
        Err(Error::NoEntry) => Ok(None),
        Err(e) => Err(e.to_string()),
    }
}

/// Anzahl Teile, wenn der Haupteintrag auf Teile verweist.
fn part_count(id: &str, kind: Kind) -> Result<usize, String> {
    Ok(get_raw(entry(id, kind)?)?.and_then(|s| s.strip_prefix(PARTS)?.parse().ok()).unwrap_or(0))
}

fn delete_parts(id: &str, kind: Kind) -> Result<(), String> {
    delete_part_range(id, kind, 1, part_count(id, kind)?)
}

fn delete_part_range(id: &str, kind: Kind, from: usize, to: usize) -> Result<(), String> {
    for i in from..=to {
        match entry_part(id, kind, i)?.delete_credential() {
            Ok(()) | Err(Error::NoEntry) => {}
            Err(e) => return Err(e.to_string()),
        }
    }
    Ok(())
}

/// Passwörter schneller Verbindungen: nur im Arbeitsspeicher, nie im Schlüsselbund.
/// Auch der Schlüssel einer gewählten Schlüsseldatei (nur Passwort und SSH-Schlüssel).
static EPHEMERAL: std::sync::LazyLock<std::sync::Mutex<std::collections::HashMap<(String, u8), String>>> = std::sync::LazyLock::new(Default::default);

/// Geheimnis im Speicher mit Nullen überschreiben, bevor es freigegeben wird (Nullbytes sind gültiges UTF-8).
fn wipe(mut s: String) {
    zeroize::Zeroize::zeroize(&mut s);
}

pub fn set_ephemeral(id: &str, kind: Kind, secret: &str) {
    if let Some(old) = EPHEMERAL.lock().unwrap().insert((id.to_string(), kind as u8), secret.to_string()) {
        wipe(old);
    }
}

pub fn get(id: &str, kind: Kind) -> Result<Option<String>, String> {
    if id.starts_with("quick-") {
        return Ok(EPHEMERAL.lock().unwrap().get(&(id.to_string(), kind as u8)).cloned());
    }
    let Some(s) = get_raw(entry(id, kind)?)? else { return Ok(None) };
    let Some(n) = s.strip_prefix(PARTS).and_then(|n| n.parse::<usize>().ok()) else { return Ok(Some(s)) };
    let mut all = String::new();
    for i in 1..=n {
        all += &get_raw(entry_part(id, kind, i)?)?.ok_or("Tresor: Teil eines Schlüssels fehlt")?;
    }
    Ok(Some(all))
}

pub fn delete(id: &str, kind: Kind) -> Result<(), String> {
    if id.starts_with("quick-") {
        if let Some(old) = EPHEMERAL.lock().unwrap().remove(&(id.to_string(), kind as u8)) {
            wipe(old);
        }
        return Ok(());
    }
    delete_parts(id, kind)?;
    match entry(id, kind)?.delete_credential() {
        Ok(()) | Err(Error::NoEntry) => Ok(()),
        Err(e) => Err(e.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schnelle_geheimnisse_ersetzen_und_loeschen() {
        set_ephemeral("quick-t1", Kind::Password, "alt");
        set_ephemeral("quick-t1", Kind::Password, "neu");
        set_ephemeral("quick-t1", Kind::SshKey, "schluessel");
        assert_eq!(get("quick-t1", Kind::Password).unwrap().as_deref(), Some("neu"), "Ersetzen statt anhängen");
        delete("quick-t1", Kind::Password).unwrap();
        assert_eq!(get("quick-t1", Kind::Password).unwrap(), None);
        assert_eq!(get("quick-t1", Kind::SshKey).unwrap().as_deref(), Some("schluessel"));
        delete("quick-t1", Kind::SshKey).unwrap();
    }

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
        // Langer Schlüssel in Teilen, danach kurzer: alte Teile verschwinden.
        let long = include_str!("testdata/keys/rsa-pkcs1-crlf.pem");
        set(&id, Kind::SshKey, long).unwrap();
        assert_eq!(get(&id, Kind::SshKey).unwrap().as_deref(), Some(long));
        set(&id, Kind::SshKey, "kurz").unwrap();
        assert_eq!(part_count(&id, Kind::SshKey).unwrap(), 0);
        assert!(get_raw(entry_part(&id, Kind::SshKey, 1).unwrap()).unwrap().is_none());
        set(&id, Kind::SshKey, long).unwrap();
        delete(&id, Kind::SshKey).unwrap();
        assert_eq!(get(&id, Kind::SshKey).unwrap(), None);
        assert!(get_raw(entry_part(&id, Kind::SshKey, 1).unwrap()).unwrap().is_none());
    }
}
