//! Gespeicherte Zugangsdaten: Benutzername + Passwort und/oder SSH-Schlüssel, mehreren Verbindungen zuweisbar.
//! Die Liste (ohne Geheimnisse) liegt als JSON neben der Bibliothek, die Geheimnisse im Tresor unter `cred-<id>`.

use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Cred {
    pub id: String,
    pub name: String,
    pub user: String,
    pub has_password: bool,
    pub has_key: bool,
}

/// Tresor-ID der Geheimnisse eines Profils.
pub fn vault_id(id: &str) -> String {
    format!("cred-{id}")
}

pub fn load(path: &Path) -> Result<Vec<Cred>, String> {
    match std::fs::read_to_string(path) {
        Ok(s) => serde_json::from_str(&s).map_err(|e| format!("Zugangsdaten-Liste beschädigt: {e}")),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(e) => Err(e.to_string()),
    }
}

pub fn save(path: &Path, items: &[Cred]) -> Result<(), String> {
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_string_pretty(items).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, path).map_err(|e| e.to_string())
}

pub fn validate(c: &Cred) -> Result<(), String> {
    if c.id.is_empty() || c.id.len() > 64 || !c.id.chars().all(|ch| ch.is_ascii_alphanumeric() || ch == '-') {
        return Err("Ungültige ID".into());
    }
    if c.name.trim().is_empty() {
        return Err("Name fehlt".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_and_validation() {
        let p = std::env::temp_dir().join(format!("sesam-creds-{}.json", std::process::id()));
        assert!(load(&p).unwrap().is_empty());
        let c = Cred { id: "a-1".into(), name: "Büro".into(), user: "mike".into(), ..Default::default() };
        validate(&c).unwrap();
        save(&p, std::slice::from_ref(&c)).unwrap();
        assert_eq!(load(&p).unwrap(), vec![c.clone()]);
        assert!(validate(&Cred { id: "../x".into(), ..c.clone() }).is_err());
        assert!(validate(&Cred { name: " ".into(), ..c }).is_err());
        assert_eq!(vault_id("a-1"), "cred-a-1");
        std::fs::remove_file(p).unwrap();
    }
}
