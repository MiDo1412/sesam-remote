//! Bibliothek, Gruppen, Sicherung, schnelle Verbindungen.

use crate::*;

pub(crate) struct LibraryPath(pub(crate) PathBuf);

/// Serialisiert Lese-Ändern-Schreiben der Bibliothek.
pub(crate) static LIBRARY_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Menüleiste in der Sprache der Oberfläche (src/i18n.ts): Rust baut sie deutsch, die Seite schickt das Wörterbuch.
#[tauri::command]
pub(crate) fn relabel_menu(app: tauri::AppHandle, map: std::collections::HashMap<String, String>) {
    #[cfg(desktop)]
    {
        use tauri::menu::MenuItemKind;
        fn walk<R: tauri::Runtime>(items: Vec<MenuItemKind<R>>, map: &std::collections::HashMap<String, String>) {
            for item in items {
                match item {
                    MenuItemKind::MenuItem(m) => {
                        if let Some(t) = m.text().ok().and_then(|t| map.get(&t)) {
                            let _ = m.set_text(t);
                        }
                    }
                    MenuItemKind::Check(m) => {
                        if let Some(t) = m.text().ok().and_then(|t| map.get(&t)) {
                            let _ = m.set_text(t);
                        }
                    }
                    MenuItemKind::Submenu(m) => {
                        if let Some(t) = m.text().ok().and_then(|t| map.get(&t)) {
                            let _ = m.set_text(t);
                        }
                        if let Ok(children) = m.items() {
                            walk(children, map);
                        }
                    }
                    _ => {}
                }
            }
        }
        if let Some(items) = app.menu().and_then(|m| m.items().ok()) {
            walk(items, &map);
        }
    }
    #[cfg(not(desktop))]
    let _ = (app, map);
}

/// iOS: Fängt Shell oder Desktop gerade die Tasten ab, blendet main.mm die Formularleiste (∧ ∨ Fertig) aus.
#[tauri::command]
pub(crate) fn key_capture(on: bool) {
    #[cfg(target_os = "ios")]
    if let Some(cls) = objc2::runtime::AnyClass::get(c"SesamKeyCapture") {
        let _: () = unsafe { objc2::msg_send![cls, setOn: objc2::runtime::Bool::new(on)] };
    }
    #[cfg(not(target_os = "ios"))]
    let _ = on;
}

#[tauri::command]
pub(crate) fn library_list(path: State<LibraryPath>) -> Result<Vec<Connection>, String> {
    let mut items = library::load(&path.0)?;
    // Mit gespeicherten Zugangsdaten gilt deren Benutzer (wie in `find`): sonst zeigte die Bibliothek nach einer
    // Änderung im Tresor noch den alten Namen.
    let creds = creds::load(&creds_path(&path)).unwrap_or_default();
    for c in &mut items {
        if let Some(cred) = c.credential_id.as_ref().and_then(|cid| creds.iter().find(|x| &x.id == cid)) {
            c.user = cred.user.clone();
        }
    }
    Ok(items)
}

/// `password`/`vnc_password`/`ssh_key`: `None` = unverändert, `Some("")` = löschen, sonst neu setzen.
#[tauri::command]
pub(crate) fn library_save(
    path: State<LibraryPath>,
    mut connection: Connection,
    password: Option<String>,
    vnc_password: Option<String>,
    rdp_password: Option<String>,
    gateway_password: Option<String>,
    ssh_key: Option<String>,
) -> Result<Connection, String> {
    connection.validate()?;
    // «quick-» gehört den schnellen Verbindungen (nur im Arbeitsspeicher), nie der Bibliothek.
    if connection.id.starts_with("quick-") {
        return Err("Ungültige ID".into());
    }
    if let Some(k) = ssh_key.as_deref().filter(|k| !k.trim().is_empty()) {
        ssh::check_private_key(k)?;
    }
    let _g = LIBRARY_LOCK.lock().unwrap();
    let mut items = library::load(&path.0)?;
    let old = items.iter().find(|c| c.id == connection.id);
    connection.has_password = old.is_some_and(|c| c.has_password);
    connection.has_vnc_password = old.is_some_and(|c| c.has_vnc_password);
    connection.has_rdp_password = old.is_some_and(|c| c.has_rdp_password);
    connection.has_gateway_password = old.is_some_and(|c| c.has_gateway_password);
    connection.has_key = old.is_some_and(|c| c.has_key);
    for (secret, kind, flag) in [
        (password, Kind::Password, &mut connection.has_password),
        (vnc_password, Kind::VncPassword, &mut connection.has_vnc_password),
        (rdp_password, Kind::RdpPassword, &mut connection.has_rdp_password),
        (gateway_password, Kind::GatewayPassword, &mut connection.has_gateway_password),
        (ssh_key, Kind::SshKey, &mut connection.has_key),
    ] {
        match secret.as_deref() {
            None => {}
            Some("") => {
                vault::delete(&connection.id, kind)?;
                *flag = false;
            }
            Some(s) => {
                vault::set(&connection.id, kind, s)?;
                *flag = true;
            }
        }
    }
    library::upsert(&mut items, connection.clone());
    library::save(&path.0, &items)?;
    Ok(connection)
}

/// Zwei Einträge desselben Geräts zusammenführen (`source` geht in `target` auf). Vorher eine Sicherung der Bibliothek.
#[tauri::command]
pub(crate) fn library_merge(path: State<LibraryPath>, target: String, source: String) -> Result<String, String> {
    let _g = LIBRARY_LOCK.lock().unwrap();
    let mut items = library::load(&path.0)?;
    let src = items.iter().find(|c| c.id == source).cloned().ok_or("Eintrag nicht gefunden")?;
    let dst = items.iter_mut().find(|c| c.id == target).ok_or("Eintrag nicht gefunden")?;
    let had_pw = vault::get(&dst.id, Kind::Password)?.is_some();
    let had_key = vault::get(&dst.id, Kind::SshKey)?.is_some();
    let taken = library::merge(dst, &src)?;
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
    std::fs::copy(&path.0, path.0.with_extension(format!("json.vor-zusammenfuehren-{stamp}"))).map_err(|e| format!("Sicherung der Bibliothek: {e}"))?;
    let copy = |kind: Kind| -> Result<(), String> {
        match vault::get(&src.id, kind)? {
            Some(v) => vault::set(&dst.id, kind, &v),
            None => Ok(()),
        }
    };
    if taken.contains(&"VNC") {
        copy(Kind::VncPassword)?;
        dst.has_vnc_password |= src.has_vnc_password;
    }
    if taken.contains(&"RDP") {
        copy(Kind::RdpPassword)?;
        copy(Kind::GatewayPassword)?;
        dst.has_rdp_password |= src.has_rdp_password;
        dst.has_gateway_password |= src.has_gateway_password;
    }
    if !had_pw {
        copy(Kind::Password)?;
        dst.has_password |= src.has_password;
    }
    if !had_key {
        copy(Kind::SshKey)?;
        dst.has_key |= src.has_key;
    }
    let name = dst.name.clone();
    library::remove(&mut items, &src.id);
    library::save(&path.0, &items)?;
    for k in [Kind::Password, Kind::SshKey, Kind::VncPassword, Kind::RdpPassword, Kind::GatewayPassword] {
        vault::delete(&src.id, k)?;
    }
    Ok(format!("«{}» ist jetzt Teil von «{name}» ({}).", src.name, taken.join(", ")))
}

/// Welche Dienste antworten? Standardports (SSH, RDP, VNC, FTP) und eingetragene, je 0,8 s, gleichzeitig.
#[tauri::command]
pub(crate) async fn probe_services(address: String, ports: Vec<u16>) -> Result<Vec<u16>, String> {
    let host = address.trim().to_string();
    if host.is_empty() || host.contains(char::is_whitespace) || host.len() > 253 {
        return Err("Ungültige Adresse".into());
    }
    let mut all = vec![22u16, 3389, 5900, 21];
    all.extend(ports.into_iter().take(8));
    all.sort_unstable();
    all.dedup();
    let mut set = tokio::task::JoinSet::new();
    for p in all {
        let h = host.clone();
        set.spawn(async move { (p, tokio::time::timeout(std::time::Duration::from_millis(800), tokio::net::TcpStream::connect((h.as_str(), p))).await.is_ok_and(|r| r.is_ok())) });
    }
    let mut open = Vec::new();
    while let Some(Ok((p, ok))) = set.join_next().await {
        if ok {
            open.push(p);
        }
    }
    open.sort_unstable();
    Ok(open)
}

#[tauri::command]
pub(crate) fn library_delete(path: State<LibraryPath>, id: String) -> Result<(), String> {
    let _g = LIBRARY_LOCK.lock().unwrap();
    let mut items = library::load(&path.0)?;
    if library::remove(&mut items, &id) {
        vault::delete(&id, Kind::Password)?;
        vault::delete(&id, Kind::SshKey)?;
        vault::delete(&id, Kind::VncPassword)?;
        vault::delete(&id, Kind::RdpPassword)?;
        vault::delete(&id, Kind::GatewayPassword)?;
        library::save(&path.0, &items)?;
    }
    Ok(())
}

pub(crate) fn groups_path(path: &LibraryPath) -> PathBuf {
    path.0.with_file_name("groups.json")
}

/// Gruppen in der Reihenfolge der Seitenleiste (auch leere), mit Farbe.
#[tauri::command]
pub(crate) fn groups_list(path: State<LibraryPath>) -> Result<Vec<library::Group>, String> {
    Ok(library::merged_groups(library::load_groups(&groups_path(&path))?, &library::load(&path.0)?))
}

/// Reihenfolge und Farben sichern; neue Namen = leere Gruppen.
#[tauri::command]
pub(crate) fn groups_save(path: State<LibraryPath>, groups: Vec<library::Group>) -> Result<(), String> {
    library::validate_groups(&groups)?;
    let _g = LIBRARY_LOCK.lock().unwrap();
    library::save(&groups_path(&path), &groups)
}

pub(crate) const SECRET_KINDS: [(Kind, &str); 5] =
    [(Kind::Password, "password"), (Kind::VncPassword, "vnc"), (Kind::RdpPassword, "rdp"), (Kind::GatewayPassword, "gateway"), (Kind::SshKey, "sshkey")];

/// Verschlüsselte Sicherung von Bibliothek, Gruppen und Zugangsdaten samt Geheimnissen in eine Datei schreiben.
#[tauri::command]
pub(crate) async fn backup_export(path: State<'_, LibraryPath>, file: String, password: String) -> Result<String, String> {
    diag::add_secret(&password);
    // Die Sicherung enthält alle Passwörter: dieselbe Bestätigung wie «Passwort anzeigen» (security-review-2026-11-11, S-07).
    // shortcut: Windows und Linux noch ohne Bestätigung, bis Windows Hello eingebaut ist (docs/offene-punkte.md).
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    tauri::async_runtime::spawn_blocking(|| auth::authenticate("eine Sicherung mit allen Passwörtern erstellen"))
        .await
        .map_err(|e| e.to_string())??;
    let conns = library::load(&path.0)?;
    let creds = creds::load(&creds_path(&path))?;
    let mut secrets = Vec::new();
    let ids = conns.iter().map(|c| c.id.clone()).chain(creds.iter().map(|c| creds::vault_id(&c.id)));
    for id in ids {
        for (k, name) in SECRET_KINDS {
            if let Some(v) = vault::get(&id, k)? {
                secrets.push((id.clone(), name.to_string(), v));
            }
        }
    }
    let c = backup::Contents {
        connections: conns.iter().map(|c| serde_json::to_value(c).map_err(|e| e.to_string())).collect::<Result<_, _>>()?,
        groups: library::load_groups(&groups_path(&path))?,
        credentials: creds,
        secrets,
    };
    let text = backup::encrypt(&c, &password)?;
    std::fs::write(&file, text).map_err(|e| format!("{file}: {e}"))?;
    Ok(format!("{} Verbindungen und {} Zugangsdaten gesichert.", c.connections.len(), c.credentials.len()))
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct BackupPreview {
    pub(crate) new_connections: Vec<String>,
    pub(crate) known_connections: Vec<String>,
    pub(crate) new_credentials: Vec<String>,
    pub(crate) known_credentials: Vec<String>,
    /// Neue Verbindungen mit Port-Weiterleitungen bzw. freigegebenem lokalem Ordner: vor dem Einlesen zeigen (S-08).
    pub(crate) with_forwards: Vec<String>,
    pub(crate) with_shared_folder: Vec<String>,
}

pub(crate) fn backup_read(file: &str, password: &str) -> Result<backup::Contents, String> {
    diag::add_secret(password);
    backup::decrypt(&std::fs::read_to_string(file).map_err(|e| format!("{file}: {e}"))?, password)
}

/// Vorschau vor dem Einlesen: was neu dazukommt, was schon da ist (gleiche ID, bleibt unverändert).
#[tauri::command]
pub(crate) fn backup_preview(path: State<LibraryPath>, file: String, password: String) -> Result<BackupPreview, String> {
    let c = backup_read(&file, &password)?;
    let have: std::collections::HashSet<_> = library::load(&path.0)?.into_iter().map(|x| x.id).collect();
    let have_c: std::collections::HashSet<_> = creds::load(&creds_path(&path))?.into_iter().map(|x| x.id).collect();
    let name = |v: &serde_json::Value| v["name"].as_str().unwrap_or("?").to_string();
    let (kc, nc): (Vec<_>, Vec<_>) = c.connections.iter().partition(|v| have.contains(v["id"].as_str().unwrap_or("")));
    let (kr, nr): (Vec<_>, Vec<_>) = c.credentials.iter().partition(|x| have_c.contains(&x.id));
    let has = |v: &serde_json::Value, k: &str| match &v[k] {
        serde_json::Value::Array(a) => !a.is_empty(),
        serde_json::Value::String(s) => !s.trim().is_empty(),
        _ => false,
    };
    let with_forwards = nc.iter().filter(|v| has(v, "forwards")).map(|v| name(v)).collect();
    let with_shared_folder = nc.iter().filter(|v| has(v, "sharedFolder")).map(|v| name(v)).collect();
    Ok(BackupPreview {
        with_forwards,
        with_shared_folder,
        new_connections: nc.into_iter().map(name).collect(),
        known_connections: kc.into_iter().map(name).collect(),
        new_credentials: nr.into_iter().map(|x| x.name.clone()).collect(),
        known_credentials: kr.into_iter().map(|x| x.name.clone()).collect(),
    })
}

/// Zusammenführen: nur Neues kommt dazu (samt Geheimnissen), Bestehendes bleibt wie es ist.
#[tauri::command]
pub(crate) fn backup_import(path: State<LibraryPath>, file: String, password: String) -> Result<String, String> {
    let mut c = backup_read(&file, &password)?;
    let _g = LIBRARY_LOCK.lock().unwrap();
    let mut conns = library::load(&path.0)?;
    let mut creds = creds::load(&creds_path(&path))?;
    let mut fresh_ids = std::collections::HashSet::new();
    let mut fresh_creds = std::collections::HashSet::new();
    let mut added = (0, 0);
    // Zugangsdaten zuerst: neue Verbindungen dürfen nur auf Zugangsdaten zeigen, die mit derselben Sicherung neu
    // dazukommen. Sonst könnte eine fremde Sicherung vorhandene Passwörter an ihren Host schicken lassen
    // (security-review-2026-11-11, S-08).
    for cr in std::mem::take(&mut c.credentials) {
        creds::validate(&cr)?;
        if !creds.iter().any(|x| x.id == cr.id) {
            fresh_ids.insert(creds::vault_id(&cr.id));
            fresh_creds.insert(cr.id.clone());
            creds.push(cr);
            added.1 += 1;
        }
    }
    let mut unlinked = 0;
    let fresh_conns = fresh_conn_ids(&c.connections, &conns);
    for v in std::mem::take(&mut c.connections) {
        let mut conn: Connection = serde_json::from_value(v).map_err(|e| format!("Verbindung nicht lesbar: {e}"))?;
        conn.validate()?;
        // Verbindungen und Zugangsdaten teilen sich den Schlüsselbund («cred-<id>»): solche IDs nie als Verbindung.
        if !conns.iter().any(|x| x.id == conn.id) {
            unlinked += import_unlink(&mut conn, &fresh_creds, &fresh_conns)?;
            fresh_ids.insert(conn.id.clone());
            conns.push(conn);
            added.0 += 1;
        }
    }
    for (id, kind, secret) in &c.secrets {
        if let (true, Some((k, _))) = (fresh_ids.contains(id), SECRET_KINDS.iter().find(|(_, n)| n == kind)) {
            // Nie ein vorhandenes Geheimnis ersetzen.
            if vault::get(id, *k)?.is_none() {
                vault::set(id, *k, secret)?;
            }
        }
    }
    let mut groups = library::load_groups(&groups_path(&path))?;
    for g in std::mem::take(&mut c.groups) {
        if !groups.iter().any(|x| x.name == g.name) {
            groups.push(g);
        }
    }
    library::validate_groups(&groups)?;
    library::save(&path.0, &conns)?;
    creds::save(&creds_path(&path), &creds)?;
    library::save(&groups_path(&path), &groups)?;
    let note = if unlinked > 0 { format!(" {unlinked} Zuweisung(en) zu Zugangsdaten, die nicht in der Sicherung stehen, wurden entfernt; bitte bei Bedarf neu zuweisen.") } else { String::new() };
    Ok(format!("{} Verbindungen und {} Zugangsdaten übernommen; Bestehendes blieb unverändert.{note}", added.0, added.1))
}

/// IDs der Verbindungen, die mit der Sicherung wirklich neu dazukommen. Eine mitgelieferte Attrappe mit der ID einer
/// vorhandenen Verbindung zählt nicht, sonst liesse sich ein lokaler Sprungserver darüber erreichen (S-08).
pub(crate) fn fresh_conn_ids(imported: &[serde_json::Value], have: &[Connection]) -> std::collections::HashSet<String> {
    imported
        .iter()
        .filter_map(|v| v.get("id")?.as_str())
        .filter(|id| !id.starts_with("cred-") && !id.starts_with("quick-") && !have.iter().any(|x| x.id == *id))
        .map(String::from)
        .collect()
}

/// Eingelesene Verbindung entschärfen: keine reservierte Kennung, Zugangsdaten nur aus derselben Sicherung.
/// Gibt die Zahl entfernter Zuweisungen zurück.
pub(crate) fn import_unlink(conn: &mut Connection, fresh_creds: &std::collections::HashSet<String>, fresh_conns: &std::collections::HashSet<String>) -> Result<usize, String> {
    // Verbindungen und Zugangsdaten teilen sich den Schlüsselbund («cred-<id>»): solche IDs nie als Verbindung.
    if conn.id.starts_with("cred-") || conn.id.starts_with("quick-") {
        return Err("Ungültige Sicherung: Verbindung mit reservierter Kennung".into());
    }
    let mut n = 0;
    // Agent-Weiterleitung gibt dem Server den lokalen SSH-Agenten; aus einer fremden Datei nie still übernehmen.
    if conn.agent_forward {
        conn.agent_forward = false;
        n += 1;
    }
    for r in [&mut conn.credential_id, &mut conn.rdp_credential_id, &mut conn.vnc_credential_id, &mut conn.gateway_credential_id] {
        if r.as_ref().is_some_and(|id| !fresh_creds.contains(id)) {
            *r = None;
            n += 1;
        }
    }
    // Sprungserver nur, wenn er mit derselben Sicherung kommt (sonst liefe der Zugang einer vorhandenen Verbindung
    // als Sprung zu einem Ziel aus der fremden Datei).
    if !conn.jump_host.is_empty() && !fresh_conns.contains(&conn.jump_host) {
        conn.jump_host.clear();
        n += 1;
    }
    Ok(n)
}

/// Gruppe umbenennen bzw. löschen (`to` = None): ändert auch die Verbindungen.
pub(crate) fn group_change(path: &LibraryPath, from: &str, to: Option<&str>) -> Result<(), String> {
    let _g = LIBRARY_LOCK.lock().unwrap();
    let mut items = library::load(&path.0)?;
    let mut groups = library::merged_groups(library::load_groups(&groups_path(path))?, &items);
    match to {
        Some(to) => library::rename_group(&mut items, &mut groups, from, to)?,
        None => library::delete_group(&mut items, &mut groups, from),
    }
    library::save(&path.0, &items)?;
    library::save(&groups_path(path), &groups)
}

#[tauri::command]
pub(crate) fn group_rename(path: State<LibraryPath>, from: String, to: String) -> Result<(), String> {
    group_change(&path, &from, Some(to.trim()))
}

#[tauri::command]
pub(crate) fn group_delete(path: State<LibraryPath>, name: String) -> Result<(), String> {
    group_change(&path, &name, None)
}

/// «Zuletzt verbunden» setzen (nach erfolgreichem Verbinden; Fehler sind egal).
pub(crate) fn touch(path: &LibraryPath, id: &str) {
    let _g = LIBRARY_LOCK.lock().unwrap();
    let Ok(mut items) = library::load(&path.0) else { return };
    if let Some(c) = items.iter_mut().find(|c| c.id == id) {
        c.last_used = Some(chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true));
        let _ = library::save(&path.0, &items);
    }
}

pub(crate) fn new_sid(id: &str) -> String {
    format!("{id}-{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos())
}

/// Schnelle Verbindungen: nicht in der Bibliothek, nur für diese Sitzung der App.
/// Gewählte Schlüsseldatei lesen und prüfen; der Inhalt kommt ins Formular und wird wie ein eingefügter Schlüssel
/// verschlüsselt gespeichert. Nur gültige Schlüssel kommen zurück, also kein allgemeines Lesen beliebiger Dateien.
#[tauri::command]
pub(crate) fn read_key_file(path: String) -> Result<String, String> {
    use std::io::Read;
    let mut buf = Vec::new();
    std::fs::File::open(&path).and_then(|f| f.take(64 * 1024).read_to_end(&mut buf)).map_err(|e| format!("Schlüsseldatei nicht lesbar: {e}"))?;
    let key = String::from_utf8(buf).map_err(|_| "Das ist keine Textdatei und damit kein SSH-Schlüssel.".to_string())?;
    ssh::check_private_key(&key)?;
    Ok(key)
}

pub(crate) static QUICK: Mutex<Vec<Connection>> = Mutex::new(Vec::new());
pub(crate) const QUICK_MAX: usize = 20;

/// Schnelle Verbindung anlegen (nichts wird gespeichert). Gibt die ID zurück.
#[tauri::command]
/// `key_file`: Pfad einer privaten Schlüsseldatei (OpenSSH, PEM oder PuTTY .ppk), nur im Arbeitsspeicher gehalten.
/// Gespeicherte Zugangsdaten (`credential_id`) liefern Schlüssel/Passwort aus dem Tresor; ohne Benutzer auch diesen.
pub(crate) fn quick_register(path: State<LibraryPath>, mut connection: Connection, password: Option<String>, key_file: Option<String>) -> Result<String, String> {
    connection.id = format!("quick-{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos());
    connection.validate()?;
    if let Some(cid) = &connection.credential_id {
        let cred = creds::load(&creds_path(&path))?.into_iter().find(|x| &x.id == cid).ok_or("Diese Zugangsdaten gibt es nicht mehr")?;
        if connection.user.trim().is_empty() {
            connection.user = cred.user;
        }
    }
    if let Some(pw) = password.filter(|p| !p.is_empty()) {
        diag::add_secret(&pw);
        vault::set_ephemeral(&connection.id, Kind::Password, &pw);
        connection.has_password = true;
    }
    if let Some(f) = key_file.filter(|f| !f.is_empty()) {
        let key = read_key_file(f)?;
        vault::set_ephemeral(&connection.id, Kind::SshKey, &key);
        connection.has_key = true;
    }
    let id = connection.id.clone();
    let mut q = QUICK.lock().unwrap();
    q.push(connection);
    // shortcut: die Oberfläche meldet nicht, wann eine schnelle Verbindung fertig ist; darum nur die letzten
    // QUICK_MAX behalten (samt Geheimnissen). Ein Abmelden beim Schliessen der Tabs wäre genauer.
    while q.len() > QUICK_MAX {
        let old = q.remove(0);
        let _ = vault::delete(&old.id, Kind::Password);
        let _ = vault::delete(&old.id, Kind::SshKey);
    }
    Ok(id)
}
