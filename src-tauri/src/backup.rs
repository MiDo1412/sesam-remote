//! Verschlüsselte Sicherung von Bibliothek, Gruppen und Zugangsdaten samt Geheimnissen (Datei `.sesam`).
//! Schlüssel aus dem Passwort mit Argon2id, verschlüsselt mit AES-256-GCM. Ohne Passwort nicht lesbar;
//! die Geheimnisse liegen nur in dieser Datei und im Schlüsselbund, nie im Klartext auf der Platte.

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use base64::Engine;
use base64::engine::general_purpose::STANDARD as B64;
use serde::{Deserialize, Serialize};

/// Inhalt einer Sicherung (vor dem Verschlüsseln).
#[derive(Serialize, Deserialize, Default)]
pub struct Contents {
    pub connections: Vec<serde_json::Value>,
    pub groups: Vec<crate::library::Group>,
    pub credentials: Vec<crate::creds::Cred>,
    /// (Tresor-ID, Art, Geheimnis), Art wie in `kind_name`.
    pub secrets: Vec<(String, String, String)>,
}

#[derive(Serialize, Deserialize)]
struct File {
    format: String,
    salt: String,
    nonce: String,
    data: String,
}

const FORMAT: &str = "sesam-backup-1";

fn key(password: &str, salt: &[u8]) -> Result<[u8; 32], String> {
    let mut k = [0u8; 32];
    argon2::Argon2::default().hash_password_into(password.as_bytes(), salt, &mut k).map_err(|e| format!("Schlüssel: {e}"))?;
    Ok(k)
}

pub fn encrypt(c: &Contents, password: &str) -> Result<String, String> {
    if password.chars().count() < 8 {
        return Err("Das Passwort braucht mindestens 8 Zeichen".into());
    }
    let (mut salt, mut nonce) = ([0u8; 16], [0u8; 12]);
    getrandom::fill(&mut salt).and_then(|_| getrandom::fill(&mut nonce)).map_err(|e| e.to_string())?;
    let plain = serde_json::to_vec(c).map_err(|e| e.to_string())?;
    let cipher = Aes256Gcm::new_from_slice(&key(password, &salt)?).map_err(|e| e.to_string())?;
    let data = cipher.encrypt(&Nonce::from(nonce), plain.as_slice()).map_err(|_| "Verschlüsseln fehlgeschlagen".to_string())?;
    serde_json::to_string_pretty(&File { format: FORMAT.into(), salt: B64.encode(salt), nonce: B64.encode(nonce), data: B64.encode(data) }).map_err(|e| e.to_string())
}

pub fn decrypt(text: &str, password: &str) -> Result<Contents, String> {
    let f: File = serde_json::from_str(text).map_err(|_| "Keine Sesam-Sicherung".to_string())?;
    if f.format != FORMAT {
        return Err(format!("Unbekanntes Format «{}»", f.format));
    }
    let b = |s: &str| B64.decode(s).map_err(|_| "Sicherung beschädigt".to_string());
    let (salt, nonce, data) = (b(&f.salt)?, b(&f.nonce)?, b(&f.data)?);
    let nonce: [u8; 12] = nonce.try_into().map_err(|_| "Sicherung beschädigt")?;
    let cipher = Aes256Gcm::new_from_slice(&key(password, &salt)?).map_err(|e| e.to_string())?;
    let plain = cipher.decrypt(&Nonce::from(nonce), data.as_slice()).map_err(|_| "Passwort falsch oder Datei verändert".to_string())?;
    serde_json::from_slice(&plain).map_err(|e| format!("Sicherung nicht lesbar: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hin_und_zurueck_nur_mit_richtigem_passwort() {
        let c = Contents { secrets: vec![("cred-a".into(), "password".into(), "geheim äöü".into())], ..Default::default() };
        let t = encrypt(&c, "lang genug!").unwrap();
        assert!(!t.contains("geheim"));
        assert_eq!(decrypt(&t, "lang genug!").unwrap().secrets, c.secrets);
        assert!(decrypt(&t, "falsches pw").is_err());
        assert!(encrypt(&c, "kurz").is_err());
        let mut f: serde_json::Value = serde_json::from_str(&t).unwrap();
        f["data"] = serde_json::Value::String(B64.encode(b"verfaelscht"));
        assert!(decrypt(&f.to_string(), "lang genug!").is_err());
    }
}
