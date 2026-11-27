//! Abgleich über einen frei gewählten Ordner (iCloud Drive, OneDrive, Nextcloud, Syncthing …): eine mit dem
//! Abgleich-Passwort verschlüsselte Datei `sesam-sync.sesam` (Format wie die Sicherung, src/backup.rs).
//! Zusammengeführt wird dreiseitig gegen den Stand des letzten Abgleichs: was nur eine Seite geändert hat, gewinnt;
//! haben beide denselben Eintrag geändert, gewinnt die neuere Änderung (`updatedAt`), bei Gleichstand dieses Gerät.

use crate::backup::{self, Contents};
use serde_json::Value;
use std::collections::HashMap;
use std::path::Path;

pub const FILE: &str = "sesam-sync.sesam";

/// Vergleich ohne «zuletzt benutzt» (ändert sich bei jedem Verbinden, ist keine Änderung am Eintrag).
fn same(a: Option<&Value>, b: Option<&Value>) -> bool {
    let strip = |v: &Value| {
        let mut v = v.clone();
        if let Some(o) = v.as_object_mut() {
            o.remove("lastUsed");
        }
        v
    };
    match (a, b) {
        (Some(a), Some(b)) => strip(a) == strip(b),
        (None, None) => true,
        _ => false,
    }
}

fn stamp(v: Option<&Value>) -> u64 {
    v.and_then(|v| v.get("updatedAt")).and_then(Value::as_u64).unwrap_or(0)
}

/// Dreiseitig je Schlüssel; Reihenfolge wie lokal, Neues von der anderen Seite hinten an.
pub fn merge_items(base: &[Value], local: &[Value], remote: &[Value], key: &str) -> Vec<Value> {
    let k = |v: &Value| v.get(key).and_then(Value::as_str).unwrap_or_default().to_string();
    let index = |l: &[Value]| l.iter().map(|v| (k(v), v.clone())).collect::<HashMap<_, _>>();
    let (b, l, r) = (index(base), index(local), index(remote));
    let mut keys: Vec<String> = local.iter().map(k).collect();
    keys.extend(remote.iter().map(k).filter(|x| !l.contains_key(x)));
    keys.extend(base.iter().map(k).filter(|x| !l.contains_key(x) && !r.contains_key(x)));
    let mut out = Vec::new();
    for id in keys {
        let (bv, lv, rv) = (b.get(&id), l.get(&id), r.get(&id));
        let pick = if same(lv, rv) || same(rv, bv) {
            lv
        } else if same(lv, bv) {
            rv
        } else if lv.is_none() || rv.is_none() {
            // Gelöscht auf einer Seite, geändert auf der anderen: die Änderung bleibt.
            lv.or(rv)
        } else if stamp(rv) > stamp(lv) {
            rv
        } else {
            lv
        };
        if let Some(v) = pick {
            let mut v = v.clone();
            // «Zuletzt benutzt»: der spätere Wert von beiden Seiten.
            let used = [lv, rv].iter().filter_map(|x| x.and_then(|x| x.get("lastUsed")).filter(|u| !u.is_null()).cloned()).max_by(|a, b| match (a.as_u64(), b.as_u64()) {
                (Some(x), Some(y)) => x.cmp(&y),
                _ => a.as_str().cmp(&b.as_str()),
            });
            if let (Some(o), Some(u)) = (v.as_object_mut(), used) {
                o.insert("lastUsed".into(), u);
            }
            out.push(v);
        }
    }
    out
}

type Secrets = Vec<(String, String, String)>;

fn merge_secrets(base: &Secrets, local: &Secrets, remote: &Secrets) -> Secrets {
    let as_values = |s: &Secrets| s.iter().map(|(id, kind, v)| serde_json::json!({ "key": format!("{id}\u{1f}{kind}"), "id": id, "kind": kind, "v": v })).collect::<Vec<_>>();
    merge_items(&as_values(base), &as_values(local), &as_values(remote), "key")
        .into_iter()
        .map(|v| (v["id"].as_str().unwrap_or_default().into(), v["kind"].as_str().unwrap_or_default().into(), v["v"].as_str().unwrap_or_default().into()))
        .collect()
}

pub fn merge(base: &Contents, local: &Contents, remote: &Contents) -> Result<Contents, String> {
    let vals = |x: &dyn erased::Json| x.json();
    let groups = merge_items(&vals(&base.groups)?, &vals(&local.groups)?, &vals(&remote.groups)?, "name");
    let creds = merge_items(&vals(&base.credentials)?, &vals(&local.credentials)?, &vals(&remote.credentials)?, "id");
    Ok(Contents {
        connections: merge_items(&base.connections, &local.connections, &remote.connections, "id"),
        groups: groups.into_iter().map(serde_json::from_value).collect::<Result<_, _>>().map_err(|e| e.to_string())?,
        credentials: creds.into_iter().map(serde_json::from_value).collect::<Result<_, _>>().map_err(|e| e.to_string())?,
        secrets: merge_secrets(&base.secrets, &local.secrets, &remote.secrets),
    })
}

mod erased {
    /// Liste beliebiger serialisierbarer Einträge als JSON-Werte.
    pub trait Json {
        fn json(&self) -> Result<Vec<serde_json::Value>, String>;
    }
    impl<T: serde::Serialize> Json for Vec<T> {
        fn json(&self) -> Result<Vec<serde_json::Value>, String> {
            self.iter().map(|x| serde_json::to_value(x).map_err(|e| e.to_string())).collect()
        }
    }
}

fn read(path: &Path, password: &str) -> Result<Option<Contents>, String> {
    match std::fs::read_to_string(path) {
        Ok(t) => backup::decrypt(&t, password).map(Some),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("{}: {e}", path.display())),
    }
}

/// Erst in eine Nachbardatei, dann umbenennen: ein Sync-Programm sieht nie eine halbe Datei.
fn write(path: &Path, c: &Contents, password: &str) -> Result<(), String> {
    let tmp = path.with_extension("sesam.tmp");
    std::fs::write(&tmp, backup::encrypt(c, password)?).map_err(|e| format!("{}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, path).map_err(|e| format!("{}: {e}", path.display()))
}

/// Ein Abgleich. `base` ist die Datei mit dem Stand des letzten Abgleichs (auf diesem Gerät, auch verschlüsselt).
/// Liefert den zusammengeführten Stand; der Ordner und `base` sind danach auf diesem Stand.
pub fn run(folder: &Path, base: &Path, password: &str, local: &Contents) -> Result<Contents, String> {
    if !folder.is_dir() {
        return Err(format!("Ordner für den Abgleich fehlt: {}", folder.display()));
    }
    let shared = folder.join(FILE);
    let remote = read(&shared, password).map_err(|e| if e.contains("Passwort falsch") { "Das Passwort für den Abgleich passt nicht zur Datei im Ordner.".into() } else { e })?;
    let merged = match &remote {
        None => clone(local)?,
        Some(r) => merge(&read(base, password).ok().flatten().unwrap_or_default(), local, r)?,
    };
    write(&shared, &merged, password)?;
    write(base, &merged, password)?;
    Ok(merged)
}

fn clone(c: &Contents) -> Result<Contents, String> {
    serde_json::from_value(serde_json::to_value(c).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    #[test]
    fn dreiseitig_zusammenfuehren() {
        let base = vec![json!({"id": "a", "name": "A"}), json!({"id": "b", "name": "B"}), json!({"id": "c", "name": "C"}), json!({"id": "d", "name": "D", "updatedAt": 1})];
        // hier: A umbenannt, C gelöscht, E neu, D geändert (älter); dort: B geändert, D geändert (neuer), F neu
        let local = vec![json!({"id": "a", "name": "A2"}), json!({"id": "b", "name": "B"}), json!({"id": "d", "name": "D-hier", "updatedAt": 5}), json!({"id": "e", "name": "E"})];
        let remote = vec![json!({"id": "a", "name": "A", "lastUsed": 9}), json!({"id": "b", "name": "B2"}), json!({"id": "c", "name": "C"}), json!({"id": "d", "name": "D-dort", "updatedAt": 7}), json!({"id": "f", "name": "F"})];
        let m = super::merge_items(&base, &local, &remote, "id");
        let names: Vec<_> = m.iter().map(|v| v["name"].as_str().unwrap()).collect();
        assert_eq!(names, ["A2", "B2", "D-dort", "E", "F"]);
        assert_eq!(m[0]["lastUsed"], 9);
        // Ohne gemeinsamen Stand (erster Abgleich eines Geräts): Vereinigung, nichts geht verloren.
        let m = super::merge_items(&[], &local, &remote, "id");
        assert_eq!(m.len(), 6);
        // Gelöscht dort, hier geändert: bleibt.
        let m = super::merge_items(&base[..1], &[json!({"id": "a", "name": "A3"})], &[], "id");
        assert_eq!(m[0]["name"], "A3");
    }

    #[test]
    fn ordner_hin_und_zurueck() {
        let dir = std::env::temp_dir().join(format!("sesam-sync-{}", std::process::id()));
        let (one, two) = (dir.join("eins"), dir.join("zwei"));
        let shared = dir.join("ordner");
        for d in [&one, &two, &shared] {
            std::fs::create_dir_all(d).unwrap();
        }
        let pw = "langes passwort";
        let mut a = super::Contents::default();
        a.connections.push(json!({"id": "x", "name": "Server"}));
        a.secrets.push(("x".into(), "password".into(), "geheim".into()));
        let first = super::run(&shared, &one.join("base"), pw, &a).unwrap();
        assert_eq!(first.connections.len(), 1);
        // Zweites Gerät mit eigenem Eintrag bekommt beide.
        let mut b = super::Contents::default();
        b.connections.push(json!({"id": "y", "name": "NAS"}));
        let second = super::run(&shared, &two.join("base"), pw, &b).unwrap();
        assert_eq!(second.connections.len(), 2);
        assert_eq!(second.secrets, vec![("x".to_string(), "password".to_string(), "geheim".to_string())]);
        // Erstes Gerät löscht «x»: nach dem Abgleich ist er überall weg, «y» kommt dazu.
        let mut a2 = super::Contents::default();
        a2.connections.push(json!({"id": "y", "name": "NAS"}));
        let third = super::run(&shared, &one.join("base"), pw, &a2).unwrap();
        assert_eq!(third.connections.len(), 1);
        assert!(third.secrets.is_empty());
        assert!(super::run(&shared, &dir.join("base3"), "falsches passwort", &a2).err().unwrap().contains("passt nicht"));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
