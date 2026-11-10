mod backup;
mod clipshare;
mod creds;
mod diag;
mod h264;
mod kbhook;
mod kdc;
mod firstaid;
mod forward;
mod unlock;
mod ftp;
mod library;
mod perms;
mod localterm;
mod profiles;
mod rdp;
mod relay;
mod rdp_redir;
#[cfg(not(windows))]
mod rdpclip;
mod rdstls;
mod scp;
#[cfg(target_os = "macos")]
mod sfsymbol;
mod sftp;
mod ssh;
mod vault;
mod wake;
mod vnc;

use library::Connection;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tauri::ipc::{Channel, InvokeResponseBody};
use tauri::{AppHandle, Emitter, Manager, State};
use clipshare::{ClipShare, Last, Local};
use vault::Kind;

struct LibraryPath(PathBuf);

/// Serialisiert Lese-Ändern-Schreiben der Bibliothek.
static LIBRARY_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[tauri::command]
fn library_list(path: State<LibraryPath>) -> Result<Vec<Connection>, String> {
    library::load(&path.0)
}

/// `password`/`vnc_password`/`ssh_key`: `None` = unverändert, `Some("")` = löschen, sonst neu setzen.
#[tauri::command]
fn library_save(
    path: State<LibraryPath>,
    mut connection: Connection,
    password: Option<String>,
    vnc_password: Option<String>,
    rdp_password: Option<String>,
    gateway_password: Option<String>,
    ssh_key: Option<String>,
) -> Result<Connection, String> {
    connection.validate()?;
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
fn library_merge(path: State<LibraryPath>, target: String, source: String) -> Result<String, String> {
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
async fn probe_services(address: String, ports: Vec<u16>) -> Result<Vec<u16>, String> {
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
fn library_delete(path: State<LibraryPath>, id: String) -> Result<(), String> {
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

fn groups_path(path: &LibraryPath) -> PathBuf {
    path.0.with_file_name("groups.json")
}

/// Gruppen in der Reihenfolge der Seitenleiste (auch leere), mit Farbe.
#[tauri::command]
fn groups_list(path: State<LibraryPath>) -> Result<Vec<library::Group>, String> {
    Ok(library::merged_groups(library::load_groups(&groups_path(&path))?, &library::load(&path.0)?))
}

/// Reihenfolge und Farben sichern; neue Namen = leere Gruppen.
#[tauri::command]
fn groups_save(path: State<LibraryPath>, groups: Vec<library::Group>) -> Result<(), String> {
    library::validate_groups(&groups)?;
    let _g = LIBRARY_LOCK.lock().unwrap();
    library::save(&groups_path(&path), &groups)
}

const SECRET_KINDS: [(Kind, &str); 5] =
    [(Kind::Password, "password"), (Kind::VncPassword, "vnc"), (Kind::RdpPassword, "rdp"), (Kind::GatewayPassword, "gateway"), (Kind::SshKey, "sshkey")];

/// Verschlüsselte Sicherung von Bibliothek, Gruppen und Zugangsdaten samt Geheimnissen in eine Datei schreiben.
#[tauri::command]
fn backup_export(path: State<LibraryPath>, file: String, password: String) -> Result<String, String> {
    diag::add_secret(&password);
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
struct BackupPreview {
    new_connections: Vec<String>,
    known_connections: Vec<String>,
    new_credentials: Vec<String>,
    known_credentials: Vec<String>,
}

fn backup_read(file: &str, password: &str) -> Result<backup::Contents, String> {
    diag::add_secret(password);
    backup::decrypt(&std::fs::read_to_string(file).map_err(|e| format!("{file}: {e}"))?, password)
}

/// Vorschau vor dem Einlesen: was neu dazukommt, was schon da ist (gleiche ID, bleibt unverändert).
#[tauri::command]
fn backup_preview(path: State<LibraryPath>, file: String, password: String) -> Result<BackupPreview, String> {
    let c = backup_read(&file, &password)?;
    let have: std::collections::HashSet<_> = library::load(&path.0)?.into_iter().map(|x| x.id).collect();
    let have_c: std::collections::HashSet<_> = creds::load(&creds_path(&path))?.into_iter().map(|x| x.id).collect();
    let name = |v: &serde_json::Value| v["name"].as_str().unwrap_or("?").to_string();
    let (kc, nc): (Vec<_>, Vec<_>) = c.connections.iter().partition(|v| have.contains(v["id"].as_str().unwrap_or("")));
    let (kr, nr): (Vec<_>, Vec<_>) = c.credentials.iter().partition(|x| have_c.contains(&x.id));
    Ok(BackupPreview {
        new_connections: nc.into_iter().map(name).collect(),
        known_connections: kc.into_iter().map(name).collect(),
        new_credentials: nr.into_iter().map(|x| x.name.clone()).collect(),
        known_credentials: kr.into_iter().map(|x| x.name.clone()).collect(),
    })
}

/// Zusammenführen: nur Neues kommt dazu (samt Geheimnissen), Bestehendes bleibt wie es ist.
#[tauri::command]
fn backup_import(path: State<LibraryPath>, file: String, password: String) -> Result<String, String> {
    let c = backup_read(&file, &password)?;
    let _g = LIBRARY_LOCK.lock().unwrap();
    let mut conns = library::load(&path.0)?;
    let mut creds = creds::load(&creds_path(&path))?;
    let mut fresh_ids = std::collections::HashSet::new();
    let mut added = (0, 0);
    for v in c.connections {
        let conn: Connection = serde_json::from_value(v).map_err(|e| format!("Verbindung nicht lesbar: {e}"))?;
        conn.validate()?;
        if !conns.iter().any(|x| x.id == conn.id) {
            fresh_ids.insert(conn.id.clone());
            conns.push(conn);
            added.0 += 1;
        }
    }
    for cr in c.credentials {
        creds::validate(&cr)?;
        if !creds.iter().any(|x| x.id == cr.id) {
            fresh_ids.insert(creds::vault_id(&cr.id));
            creds.push(cr);
            added.1 += 1;
        }
    }
    for (id, kind, secret) in c.secrets {
        if let (true, Some((k, _))) = (fresh_ids.contains(&id), SECRET_KINDS.iter().find(|(_, n)| *n == kind)) {
            vault::set(&id, *k, &secret)?;
        }
    }
    let mut groups = library::load_groups(&groups_path(&path))?;
    for g in c.groups {
        if !groups.iter().any(|x| x.name == g.name) {
            groups.push(g);
        }
    }
    library::validate_groups(&groups)?;
    library::save(&path.0, &conns)?;
    creds::save(&creds_path(&path), &creds)?;
    library::save(&groups_path(&path), &groups)?;
    Ok(format!("{} Verbindungen und {} Zugangsdaten übernommen; Bestehendes blieb unverändert.", added.0, added.1))
}

/// Gruppe umbenennen bzw. löschen (`to` = None): ändert auch die Verbindungen.
fn group_change(path: &LibraryPath, from: &str, to: Option<&str>) -> Result<(), String> {
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
fn group_rename(path: State<LibraryPath>, from: String, to: String) -> Result<(), String> {
    group_change(&path, &from, Some(to.trim()))
}

#[tauri::command]
fn group_delete(path: State<LibraryPath>, name: String) -> Result<(), String> {
    group_change(&path, &name, None)
}

/// «Zuletzt verbunden» setzen (nach erfolgreichem Verbinden; Fehler sind egal).
fn touch(path: &LibraryPath, id: &str) {
    let _g = LIBRARY_LOCK.lock().unwrap();
    let Ok(mut items) = library::load(&path.0) else { return };
    if let Some(c) = items.iter_mut().find(|c| c.id == id) {
        c.last_used = Some(chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true));
        let _ = library::save(&path.0, &items);
    }
}

fn new_sid(id: &str) -> String {
    format!("{id}-{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos())
}

/// Schnelle Verbindungen: nicht in der Bibliothek, nur für diese Sitzung der App.
static QUICK: Mutex<Vec<Connection>> = Mutex::new(Vec::new());

/// Schnelle Verbindung anlegen (nichts wird gespeichert). Gibt die ID zurück.
#[tauri::command]
/// `key_file`: Pfad einer privaten Schlüsseldatei (OpenSSH oder PuTTY .ppk), nur im Arbeitsspeicher gehalten.
/// Gespeicherte Zugangsdaten (`credential_id`) liefern Schlüssel/Passwort aus dem Tresor; ohne Benutzer auch diesen.
fn quick_register(path: State<LibraryPath>, mut connection: Connection, password: Option<String>, key_file: Option<String>) -> Result<String, String> {
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
        let key = std::fs::read_to_string(&f).map_err(|e| format!("Schlüsseldatei nicht lesbar: {e}"))?;
        ssh::check_private_key(&key)?;
        vault::set_ephemeral(&connection.id, Kind::SshKey, &key);
        connection.has_key = true;
    }
    let id = connection.id.clone();
    QUICK.lock().unwrap().push(connection);
    Ok(id)
}

fn find(path: &LibraryPath, id: &str) -> Result<Connection, String> {
    if id.starts_with("quick-") {
        return QUICK.lock().unwrap().iter().find(|c| c.id == id).cloned().ok_or_else(|| "Schnelle Verbindung nicht mehr vorhanden".into());
    }
    let mut c = find_raw(path, id)?;
    // Gespeicherte Zugangsdaten: Benutzername von dort (Geheimnisse über login_secret).
    if let Some(cid) = &c.credential_id {
        let cred = creds::load(&creds_path(path))?.into_iter().find(|x| &x.id == cid).ok_or("Zugewiesene Zugangsdaten gibt es nicht mehr")?;
        c.user = cred.user;
    }
    Ok(c)
}

/// RDP-Benutzer und -Passwort: gespeicherte Zugangsdaten für RDP, eigene für RDP, sonst die der Verbindung.
fn rdp_login(path: &LibraryPath, c: &Connection) -> Result<(String, String), String> {
    Ok(match &c.rdp_credential_id {
        Some(cid) => {
            let (u, p) = cred_login(path, cid)?;
            (u, p.unwrap_or_default())
        }
        None => (
            match c.rdp_user.trim() {
                "" => c.user.clone(),
                u => u.to_string(),
            },
            vault::get(&c.id, Kind::RdpPassword)?.or(login_secret(c, Kind::Password)?).unwrap_or_default(),
        ),
    })
}

/// «Zugangsdaten prüfen»: nur bis zur Anmeldung (NLA), dann sauber trennen. Zertifikat muss schon bestätigt sein.
#[tauri::command]
async fn rdp_check(path: State<'_, LibraryPath>, id: String, trust: Option<String>, remember: Option<bool>) -> Result<(), String> {
    let c = find(&path, &id)?;
    let port = c.rdp_port.ok_or("RDP ist für diese Verbindung ausgeschaltet")?;
    let (user, password) = rdp_login(&path, &c)?;
    let pins_path = path.0.with_file_name("known_certs.json");
    let mut pins = load_pins(&pins_path);
    let pin_key = format!("{}:{port}", c.address);
    let t = rdp::Target {
        host: c.address.clone(),
        port,
        user,
        domain: (!c.rdp_domain.trim().is_empty()).then(|| c.rdp_domain.trim().to_string()),
        password,
        expected_cert: trust.clone().or_else(|| pins.get(&pin_key).cloned()),
        skip_cert_check: c.skip_cert_check,
        width: 1024,
        height: 768,
        scale: 100,
        clipboard: None,
        audio: false,
        share: None,
        kdc: kdc::kdc_url(&c.rdp_kdc)?,
        no_ntlm: c.rdp_no_ntlm,
        gateway: None,
    };
    let seen: rdp::SeenCert = Default::default();
    rdp::check_login(t, seen.clone()).await?;
    // Wie beim Verbinden: bestätigtes Zertifikat merken, damit die nächste Verbindung nicht erneut fragt.
    if let Some(fp) = trust.filter(|fp| remember.unwrap_or(true) && seen.lock().unwrap().as_ref() == Some(fp)) {
        pins.insert(pin_key, fp);
        let _ = std::fs::write(&pins_path, serde_json::to_string_pretty(&pins).unwrap_or_default());
    }
    Ok(())
}

/// Erste Hilfe: Verbindung für einen Dienst (ssh, rdp, vnc, ftp) Schritt für Schritt prüfen; jeder Schritt geht
/// sofort über `on_step` an die Oberfläche. Anmeldung und Zertifikat mit den gespeicherten Zugangsdaten,
/// ohne etwas zu bestätigen oder zu speichern.
#[tauri::command]
async fn conn_diagnose(path: State<'_, LibraryPath>, id: String, service: String, on_step: Channel<firstaid::Step>) -> Result<(), String> {
    use firstaid::step;
    let c = find(&path, &id)?;
    let port = match service.as_str() {
        "ssh" => c.ssh_port,
        "rdp" => c.rdp_port,
        "vnc" => c.vnc_port,
        "ftp" => c.ftp_port,
        _ => return Err("Unbekannter Dienst".into()),
    }
    .ok_or_else(|| format!("{} ist für «{}» ausgeschaltet", service.to_uppercase(), c.name))?;
    let send = |s: firstaid::Step| {
        let _ = on_step.send(s);
    };
    if !c.jump_host.is_empty() && service == "ssh" {
        send(step("name", "Sprungserver", None, "Die Verbindung läuft über einen Sprungserver; geprüft wird nur die Anmeldung.", ""));
    } else if c.ssh_tunnel && service != "ssh" {
        send(step("name", "SSH-Tunnel", None, "Der Desktop läuft durch SSH; geprüft wird zuerst SSH.", ""));
        if !firstaid::network(&c.address, c.ssh_port.unwrap_or(22), "ssh", &send).await {
            return Ok(());
        }
    } else if !firstaid::network(&c.address, port, &service, &send).await {
        return Ok(());
    }
    let login = "Anmeldung";
    match service.as_str() {
        "ssh" => match ssh::connect(&ssh_target(&c, None)?).await {
            Ok(h) => {
                send(step("login", login, Some(true), format!("Angemeldet als {}.", c.user), ""));
                tokio::spawn(async move { h.disconnect(russh::Disconnect::ByApplication, "", "de").await });
            }
            Err(e) if e.starts_with(ssh::HOSTKEY_UNKNOWN) => {
                send(step("cert", "Hostschlüssel", None, "Noch nicht bestätigt. Beim ersten Verbinden zeigt Sesam den Fingerabdruck zum Vergleichen.", "trust"));
            }
            Err(e) => send(step("login", login, Some(false), e, "creds")),
        },
        "rdp" => match rdp_check(path, id, None, Some(false)).await {
            Ok(()) => {
                send(step("cert", "Zertifikat", Some(true), "Bekannt oder gültig.", ""));
                send(step("login", login, Some(true), "Anmeldung angenommen.", ""));
            }
            Err(e) if e.starts_with(vnc::tls::CERT_UNKNOWN) => {
                send(step("cert", "Zertifikat", None, "Eigenes Zertifikat, noch nicht bestätigt. Beim Verbinden zeigt Sesam den Fingerabdruck.", "trust"));
            }
            Err(e) if e.starts_with(vnc::tls::CERT_CHANGED) => {
                send(step("cert", "Zertifikat", Some(false), "Hat sich seit dem letzten Mal geändert. Nur weiter, wenn es auf dem Server erneuert wurde.", "trust"));
            }
            Err(e) => send(step("login", login, Some(false), e, "creds")),
        },
        _ => send(step("login", login, None, "Prüft Sesam beim Verbinden.", "")),
    }
    Ok(())
}

/// «Desktop freischalten», Schritt 1: System per SSH erkennen (nur lesen) und den Plan zeigen.
#[tauri::command]
async fn desktop_detect(path: State<'_, LibraryPath>, id: String, trust: Option<String>) -> Result<unlock::Plan, String> {
    let c = find(&path, &id)?;
    Ok(unlock::plan(&ssh::exec(&ssh_target(&c, trust)?, unlock::DETECT).await?))
}

/// Schritt 2, nach Bestätigung: die festen Befehle des Systems mit sudo ausführen (Ubuntu: Setup-Script läuft in der
/// Shell, hier nur eintragen) und den Desktop-Dienst in der Verbindung eintragen. `SUDO_PASSWORD`: Passwort nötig.
#[tauri::command]
async fn desktop_enable(path: State<'_, LibraryPath>, id: String, trust: Option<String>, system: String, password: Option<String>) -> Result<(), String> {
    let c = find(&path, &id)?;
    if system != "ubuntu" {
        let s = sftp::open(ssh_target(&c, trust)?).await?;
        s.exec(&unlock::script(&system)?, true, password.as_deref()).await?;
    }
    let _g = LIBRARY_LOCK.lock().unwrap();
    let mut lib = library::load(&path.0)?;
    let x = lib.iter_mut().find(|x| x.id == id).ok_or("Verbindung nicht gefunden")?;
    if system == "ubuntu" {
        x.rdp_port = Some(3389);
        x.rdp_target = "login".into();
        x.desktop_protocol = "rdp".into();
    } else {
        x.vnc_port = Some(5900);
        x.desktop_protocol = "vnc".into();
    }
    if x.os.is_empty() {
        x.os = system;
    }
    x.validate()?;
    library::save(&path.0, &lib)
}

/// Ports aus /etc/sesam-rdp/ports per SSH lesen (nur `cat`, kein sudo). Passwort aus dem Formular
/// oder aus dem Tresor der Verbindung `id`.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
async fn rdp_ports(
    path: State<'_, LibraryPath>,
    host: String,
    port: u16,
    user: String,
    password: Option<String>,
    id: Option<String>,
    trust: Option<String>,
) -> Result<Vec<(String, u16)>, String> {
    let conn = id.as_deref().and_then(|i| find(&path, i).ok());
    let password = match password.filter(|p| !p.is_empty()) {
        Some(p) => Some(p),
        None => match &conn {
            Some(c) => login_secret(c, Kind::Password)?,
            None => None,
        },
    };
    if let Some(p) = &password {
        diag::add_secret(p);
    }
    let key = match &conn {
        Some(c) => login_secret(c, Kind::SshKey)?,
        None => None,
    };
    let opts = ssh::Options { agent: conn.as_ref().is_some_and(|c| c.ssh_agent), ..Default::default() };
    let t = ssh::Target { host, port, user, password, key, trusted: trust, opts, forwards: Vec::new(), jump: None };
    let text = ssh::exec(&t, "if [ -r /etc/sesam-rdp/ports ]; then cat /etc/sesam-rdp/ports; else echo __sesam_none__; fi").await.map_err(|e| {
        if e.starts_with(ssh::HOSTKEY_UNKNOWN) { e } else { format!("Ports nicht lesbar: {e}") }
    })?;
    if text.trim() == "__sesam_none__" {
        return Err("Auf diesem PC hat das Sesam-Setup-Script keine Ports angelegt. Port von Hand eintragen: in Ubuntu unter Einstellungen › System › Remote-Desktop › Desktopfreigabe (meist 3389, mit Remote-Anmeldung oft 3390).".into());
    }
    Ok(ssh::parse_ports(&text))
}

/// Benutzer und Passwort gespeicherter Zugangsdaten (für RDP, VNC, Gateway).
fn cred_login(path: &LibraryPath, cid: &str) -> Result<(String, Option<String>), String> {
    let cred = creds::load(&creds_path(path))?.into_iter().find(|x| x.id == cid).ok_or("Zugewiesene Zugangsdaten gibt es nicht mehr")?;
    cred_used(cid);
    let pw = vault::get(&creds::vault_id(cid), Kind::Password)?;
    if let Some(p) = &pw {
        diag::add_secret(p);
    }
    Ok((cred.user, pw))
}

fn creds_path(path: &LibraryPath) -> PathBuf {
    path.0.with_file_name("credentials.json")
}

/// Zugangsdaten mit den Namen der Verbindungen, die sie nutzen.
#[tauri::command]
fn creds_list(path: State<LibraryPath>) -> Result<Vec<(creds::Cred, Vec<String>)>, String> {
    let lib = library::load(&path.0)?;
    Ok(creds::load(&creds_path(&path))?
        .into_iter()
        .map(|c| {
            let users = lib.iter().filter(|x| x.cred_ids().contains(&Some(c.id.as_str()))).map(|x| x.name.clone()).collect();
            (c, users)
        })
        .collect())
}

/// Neuen Ed25519-Schlüssel für diese Zugangsdaten erzeugen (ersetzt einen vorhandenen); liefert den öffentlichen Schlüssel.
#[tauri::command]
fn cred_generate_key(path: State<LibraryPath>, id: String) -> Result<String, String> {
    let _g = LIBRARY_LOCK.lock().unwrap();
    let p = creds_path(&path);
    let mut all = creds::load(&p)?;
    let c = all.iter_mut().find(|x| x.id == id).ok_or("Zugangsdaten nicht gefunden")?;
    let comment = format!("{}@sesam", if c.user.is_empty() { "sesam" } else { &c.user });
    let (private, public) = ssh::generate_ed25519(&comment)?;
    vault::set(&creds::vault_id(&id), Kind::SshKey, &private)?;
    c.has_key = true;
    creds::save(&p, &all)?;
    Ok(public)
}

/// Öffentlicher Schlüssel (für authorized_keys) zum gespeicherten privaten Schlüssel.
#[tauri::command]
fn cred_public_key(id: String) -> Result<String, String> {
    let k = vault::get(&creds::vault_id(&id), Kind::SshKey)?.ok_or("Diese Zugangsdaten haben keinen SSH-Schlüssel")?;
    ssh::public_key_of(&k)
}

/// Öffentlichen Schlüssel auf einem Server eintragen (wie ssh-copy-id), angemeldet mit den bisherigen Zugangsdaten der Verbindung.
#[tauri::command]
async fn cred_install_key(path: State<'_, LibraryPath>, id: String, connection: String, trust: Option<String>) -> Result<String, String> {
    let public = cred_public_key(id)?;
    let script = ssh::install_key_script(&public)?;
    let c = find(&path, &connection)?;
    let h = ssh::connect(&ssh_target(&c, trust)?).await?;
    let o = scp::Remote::run(&h, script).await?;
    let out = String::from_utf8_lossy(&o.out).trim().to_string();
    match (o.code, out.as_str()) {
        (0, "vorhanden") => Ok(format!("Der Schlüssel war auf {} schon eingetragen.", c.name)),
        (0, _) => Ok(format!("Schlüssel auf {} eingetragen (~/.ssh/authorized_keys).", c.name)),
        (code, _) => Err(format!("Nicht eingetragen (Code {code}): {}", String::from_utf8_lossy(&o.err).trim())),
    }
}

/// Gespeichertes Passwort anzeigen bzw. kopieren: nur nach Touch ID bzw. Mac-Passwort.
#[tauri::command]
async fn cred_reveal(path: State<'_, LibraryPath>, id: String) -> Result<String, String> {
    let cred = creds::load(&creds_path(&path))?.into_iter().find(|x| x.id == id).ok_or("Zugangsdaten nicht gefunden")?;
    #[cfg(target_os = "macos")]
    {
        let reason = format!("das Passwort von «{}» anzeigen", cred.name);
        tauri::async_runtime::spawn_blocking(move || sfsymbol::authenticate(&reason)).await.map_err(|e| e.to_string())??;
    }
    // shortcut: unter Windows noch ohne Windows Hello, darum dort gesperrt; mit Hello (UserConsentVerifier) freigeben.
    #[cfg(not(target_os = "macos"))]
    {
        let _ = &cred;
        return Err("Anzeigen ist auf diesem System noch nicht möglich".into());
    }
    #[allow(unreachable_code)]
    vault::get(&creds::vault_id(&id), Kind::Password)?.ok_or_else(|| "Kein Passwort gespeichert".into())
}

/// Kopie mit eigenem Namen, samt Geheimnissen.
#[tauri::command]
fn cred_duplicate(path: State<LibraryPath>, id: String, new_id: String, name: String) -> Result<creds::Cred, String> {
    let _g = LIBRARY_LOCK.lock().unwrap();
    let p = creds_path(&path);
    let mut all = creds::load(&p)?;
    let src = all.iter().find(|x| x.id == id).cloned().ok_or("Zugangsdaten nicht gefunden")?;
    let copy = creds::Cred { id: new_id, name, ..src.clone() };
    creds::validate(&copy)?;
    for kind in [Kind::Password, Kind::SshKey] {
        if let Some(sec) = vault::get(&creds::vault_id(&src.id), kind)? {
            vault::set(&creds::vault_id(&copy.id), kind, &sec)?;
        }
    }
    all.push(copy.clone());
    creds::save(&p, &all)?;
    Ok(copy)
}

/// Anlegen/ändern. `password`/`ssh_key`: None = unverändert, "" = löschen.
#[tauri::command]
fn cred_save(path: State<LibraryPath>, mut cred: creds::Cred, password: Option<String>, ssh_key: Option<String>) -> Result<creds::Cred, String> {
    creds::validate(&cred)?;
    if let Some(k) = ssh_key.as_deref().filter(|k| !k.trim().is_empty()) {
        ssh::check_private_key(k)?;
    }
    let _g = LIBRARY_LOCK.lock().unwrap();
    let p = creds_path(&path);
    let mut all = creds::load(&p)?;
    let old = all.iter().find(|x| x.id == cred.id).cloned();
    cred.has_password = old.as_ref().is_some_and(|o| o.has_password);
    cred.has_key = old.as_ref().is_some_and(|o| o.has_key);
    let vid = creds::vault_id(&cred.id);
    for (secret, kind, flag) in [(password, Kind::Password, &mut cred.has_password), (ssh_key, Kind::SshKey, &mut cred.has_key)] {
        match secret.as_deref() {
            None => {}
            Some("") => {
                vault::delete(&vid, kind)?;
                *flag = false;
            }
            Some(s) => {
                vault::set(&vid, kind, s)?;
                *flag = true;
            }
        }
    }
    match all.iter_mut().find(|x| x.id == cred.id) {
        Some(x) => *x = cred.clone(),
        None => all.push(cred.clone()),
    }
    creds::save(&p, &all)?;
    Ok(cred)
}

/// Löschen; Verbindungen, die es nutzen, fallen auf «Eigene» zurück.
#[tauri::command]
fn cred_delete(path: State<LibraryPath>, id: String) -> Result<(), String> {
    let _g = LIBRARY_LOCK.lock().unwrap();
    let p = creds_path(&path);
    let mut all = creds::load(&p)?;
    all.retain(|x| x.id != id);
    creds::save(&p, &all)?;
    let vid = creds::vault_id(&id);
    vault::delete(&vid, Kind::Password)?;
    vault::delete(&vid, Kind::SshKey)?;
    let mut lib = library::load(&path.0)?;
    for c in lib.iter_mut() {
        for slot in [&mut c.credential_id, &mut c.rdp_credential_id, &mut c.vnc_credential_id, &mut c.gateway_credential_id] {
            if slot.as_deref() == Some(id.as_str()) {
                *slot = None;
            }
        }
    }
    library::save(&path.0, &lib)
}

/// Bestehende Zugangsdaten einer Verbindung als gespeichertes Profil übernehmen und zuweisen.
#[tauri::command]
fn cred_from_connection(path: State<LibraryPath>, id: String, cred_id: String, name: String) -> Result<creds::Cred, String> {
    let _g = LIBRARY_LOCK.lock().unwrap();
    let mut lib = library::load(&path.0)?;
    let conn = lib.iter_mut().find(|c| c.id == id).ok_or("Verbindung nicht gefunden")?;
    let mut cred = creds::Cred { id: cred_id, name, user: conn.user.clone(), ..Default::default() };
    creds::validate(&cred)?;
    let vid = creds::vault_id(&cred.id);
    if let Some(pw) = vault::get(&conn.id, Kind::Password)? {
        vault::set(&vid, Kind::Password, &pw)?;
        cred.has_password = true;
    }
    if let Some(k) = vault::get(&conn.id, Kind::SshKey)? {
        vault::set(&vid, Kind::SshKey, &k)?;
        cred.has_key = true;
    }
    conn.credential_id = Some(cred.id.clone());
    let p = creds_path(&path);
    let mut all = creds::load(&p)?;
    all.push(cred.clone());
    creds::save(&p, &all)?;
    library::save(&path.0, &lib)?;
    Ok(cred)
}

fn find_raw(path: &LibraryPath, id: &str) -> Result<Connection, String> {
    library::load(&path.0)?.into_iter().find(|c| c.id == id).ok_or_else(|| "Verbindung nicht gefunden".into())
}

/// Passwort/SSH-Schlüssel: aus den gespeicherten Zugangsdaten, wenn zugewiesen, sonst aus der Verbindung.
fn login_secret(c: &Connection, kind: Kind) -> Result<Option<String>, String> {
    // Schnelle Verbindung: hier eingetippt/gewählt geht vor gespeicherten Zugangsdaten.
    if c.id.starts_with("quick-") {
        if let Some(s) = vault::get(&c.id, kind)? {
            return Ok(Some(s));
        }
    }
    match (&c.credential_id, kind) {
        (Some(cid), Kind::Password | Kind::SshKey) => {
            cred_used(cid);
            vault::get(&creds::vault_id(cid), kind)
        }
        _ => vault::get(&c.id, kind),
    }
}

/// «Zuletzt verwendet» merken. Nur wenn die Bibliothek gerade frei ist (nie warten, nie scheitern).
fn cred_used(cid: &str) {
    let (Some(lib), Ok(_g)) = (LIBRARY_FILE.get(), LIBRARY_LOCK.try_lock()) else { return };
    let p = creds_path(&LibraryPath(lib.clone()));
    let Ok(mut all) = creds::load(&p) else { return };
    if let Some(c) = all.iter_mut().find(|x| x.id == cid) {
        c.last_used = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).ok().map(|d| d.as_secs());
        let _ = creds::save(&p, &all);
    }
}

/// Zugang testen: mit diesen Zugangsdaten über SSH an einer Verbindung anmelden, die sie nutzt, und gleich wieder trennen.
#[tauri::command]
async fn cred_test(path: State<'_, LibraryPath>, connection: String, trust: Option<String>) -> Result<String, String> {
    let c = find(&path, &connection)?;
    if c.ssh_port.is_none() {
        return Err(format!("«{}» hat kein SSH; testen geht vorerst nur über SSH.", c.name));
    }
    let h = ssh::connect(&ssh_target(&c, trust)?).await?;
    let _ = h.disconnect(russh::Disconnect::ByApplication, "", "de").await;
    Ok(format!("Anmeldung an «{}» erfolgreich.", c.name))
}

/// Pfad der Bibliothek für Stellen ohne Tauri-State (Sprungserver nachschlagen).
static LIBRARY_FILE: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();

fn ssh_target(c: &Connection, trust: Option<String>) -> Result<ssh::Target, String> {
    let mut t = ssh_target_direct(c, trust)?;
    if !c.jump_host.is_empty() {
        let path = LIBRARY_FILE.get().ok_or("Bibliothek nicht geladen")?;
        let j = find(&LibraryPath(path.clone()), &c.jump_host).map_err(|_| "Sprungserver nicht mehr in der Bibliothek".to_string())?;
        // Nur eine Stufe: der Sprungserver selbst verbindet direkt.
        t.jump = Some(Box::new(ssh_target_direct(&j, None)?));
    }
    Ok(t)
}

fn ssh_target_direct(c: &Connection, trust: Option<String>) -> Result<ssh::Target, String> {
    Ok(ssh::Target {
        host: c.address.clone(),
        port: c.ssh_port.ok_or_else(|| format!("SSH ist für «{}» ausgeschaltet", c.name))?,
        user: c.user.clone(),
        password: login_secret(c, Kind::Password)?,
        key: login_secret(c, Kind::SshKey)?,
        trusted: trust,
        opts: ssh::Options {
            legacy: c.ssh_legacy,
            keepalive: c.ssh_keepalive,
            compression: c.ssh_compression,
            agent: c.ssh_agent,
            agent_forward: c.agent_forward,
        },
        forwards: Vec::new(),
        jump: None,
    })
}

#[tauri::command]
async fn shell_open(
    path: State<'_, LibraryPath>,
    shells: State<'_, ssh::Shells>,
    id: String,
    cols: u32,
    rows: u32,
    trust: Option<String>,
    slot: Option<u32>,
    on_event: Channel<ssh::ShellEvent>,
) -> Result<String, String> {
    let c = find(&path, &id)?;
    let mut t = ssh_target(&c, trust)?;
    t.forwards = c.forwards.iter().map(|l| forward::parse(l)).collect::<Result<_, _>>()?;
    let relay = relay::Relay::new(on_event);
    // Gehaltene Shell: je Verbindung und Tab-Nummer eine tmux-Sitzung, beim Neuverbinden wieder angehängt.
    let hold = (c.persist_shell == "tmux").then(|| format!("sesam-{}-{}", c.id.chars().filter(char::is_ascii_alphanumeric).take(8).collect::<String>(), slot.unwrap_or(0)));
    let shell = ssh::open_with(t, cols, rows, relay.clone(), hold.as_deref()).await?;
    let sid = new_sid(&c.id);
    touch(&path, &c.id);
    shells.0.lock().await.insert(sid.clone(), shell);
    relay::SHELLS.lock().unwrap().insert(sid.clone(), relay);
    Ok(sid)
}

#[tauri::command]
async fn shell_write(shells: State<'_, ssh::Shells>, sid: String, data: Vec<u8>) -> Result<(), String> {
    if let Some(t) = local_term(&sid) {
        return t.write(&data);
    }
    shells.0.lock().await.get(&sid).ok_or("Shell geschlossen")?.write(data).await
}

/// Lokales Terminal zu dieser Sitzungs-ID (die Shell-Befehle bedienen SSH und lokal gleich).
fn local_term(sid: &str) -> Option<Arc<localterm::Term>> {
    localterm::TERMS.lock().unwrap().get(sid).cloned()
}

/// Lokales Terminal mit einem Profil starten (PowerShell, cmd, WSL, zsh, Python …).
#[tauri::command]
fn term_open(profile: localterm::Spec, cols: u16, rows: u16, on_event: Channel<ssh::ShellEvent>) -> Result<String, String> {
    let relay = relay::Relay::new(on_event);
    let term = localterm::open(&profile, cols, rows, relay.clone())?;
    let sid = new_sid("local");
    localterm::TERMS.lock().unwrap().insert(sid.clone(), term);
    relay::SHELLS.lock().unwrap().insert(sid.clone(), relay);
    Ok(sid)
}

/// Erkanntes Betriebssystem eintragen (nur wenn noch keins gewählt ist), für das Symbol in der Bibliothek.
#[tauri::command]
fn conn_set_os(path: State<LibraryPath>, id: String, os: String) -> Result<bool, String> {
    if !["raspberry", "ubuntu", "linux"].contains(&os.as_str()) {
        return Err("Unbekanntes System".into());
    }
    let _g = LIBRARY_LOCK.lock().unwrap();
    let mut lib = library::load(&path.0)?;
    let Some(c) = lib.iter_mut().find(|c| c.id == id) else { return Ok(false) };
    if !(c.os.is_empty() || (c.os == "linux" && os != "linux")) {
        return Ok(false);
    }
    c.os = os;
    library::save(&path.0, &lib)?;
    Ok(true)
}

/// Dateien auf die Shell gezogen: über dieselbe SSH-Verbindung (eigener SFTP-Kanal) in `dir` laden
/// ("" = Home), nie überschreiben. Gibt die Pfade auf dem Server zurück.
#[tauri::command]
async fn shell_upload(shells: State<'_, ssh::Shells>, sid: String, files: Vec<String>, dir: String) -> Result<Vec<String>, String> {
    let h = shells.0.lock().await.get(&sid).ok_or("Shell geschlossen")?.handle();
    let s = Arc::new(sftp::over(&h).await?);
    let dir = if dir.trim().is_empty() { s.s.canonicalize(".").await.map_err(|e| e.to_string())? } else { dir };
    let mut out = Vec::new();
    for f in files {
        out.push(sftp::upload(&s, std::path::Path::new(&f), &dir, false, |_| {}).await?);
    }
    Ok(out)
}

/// Türöffner: Weiterleitungen je Shell zu lokalen Diensten des Servers, (Server-Port, IPv6, lokaler Port, Task).
static DOORS: std::sync::LazyLock<Mutex<HashMap<String, Vec<(u16, bool, u16, tokio::task::JoinHandle<()>)>>>> = std::sync::LazyLock::new(Default::default);

/// `port` auf dem Server (localhost) über die SSH-Verbindung dieser Shell erreichbar machen; gibt den lokalen Port
/// auf 127.0.0.1 zurück. Ist die Tür schon offen, dieselbe.
#[tauri::command]
async fn shell_door_open(shells: State<'_, ssh::Shells>, sid: String, port: u16, v6: bool) -> Result<u16, String> {
    if let Some(d) = DOORS.lock().unwrap().get(&sid).and_then(|v| v.iter().find(|d| d.0 == port && d.1 == v6).map(|d| d.2)) {
        return Ok(d);
    }
    let h = shells.0.lock().await.get(&sid).ok_or("Shell geschlossen")?.handle();
    let (local, task) = forward::tunnel_to(h, if v6 { "::1" } else { "127.0.0.1" }, port).await?;
    DOORS.lock().unwrap().entry(sid).or_default().push((port, v6, local, task));
    Ok(local)
}

/// Offene Türen dieser Shell: (Server-Port, IPv6, lokaler Port).
#[tauri::command]
fn shell_door_list(sid: String) -> Vec<(u16, bool, u16)> {
    DOORS.lock().unwrap().get(&sid).map(|v| v.iter().map(|d| (d.0, d.1, d.2)).collect()).unwrap_or_default()
}

/// Tür schliessen (`port` 0: alle dieser Shell).
#[tauri::command]
fn shell_door_close(sid: String, port: u16) {
    if let Some(v) = DOORS.lock().unwrap().get_mut(&sid) {
        v.retain(|d| {
            let keep = port != 0 && d.0 != port;
            if !keep {
                d.3.abort();
            }
            keep
        });
    }
}

/// Gehaltene Sesam-Shells (tmux) auf dem Server dieser Shell.
#[tauri::command]
async fn shell_held_list(shells: State<'_, ssh::Shells>, sid: String) -> Result<Vec<(String, bool, u64)>, String> {
    let h = shells.0.lock().await.get(&sid).ok_or("Shell geschlossen")?.handle();
    ssh::tmux_list(&h).await
}

#[tauri::command]
async fn shell_held_kill(shells: State<'_, ssh::Shells>, sid: String, name: String) -> Result<(), String> {
    let h = shells.0.lock().await.get(&sid).ok_or("Shell geschlossen")?.handle();
    ssh::tmux_kill(&h, &name).await
}

/// Shell dieser Verbindung künftig halten (tmux) bzw. nicht mehr.
#[tauri::command]
fn conn_set_persist(path: State<LibraryPath>, id: String, on: bool) -> Result<(), String> {
    let _g = LIBRARY_LOCK.lock().unwrap();
    let mut lib = library::load(&path.0)?;
    let c = lib.iter_mut().find(|c| c.id == id).ok_or("Verbindung nicht gefunden")?;
    c.persist_shell = if on { "tmux".into() } else { String::new() };
    library::save(&path.0, &lib)
}

/// Rechner der Shell befragen (Raspberry-Pi-Erkennung, Temperatur, Unterspannung).
#[tauri::command]
async fn shell_probe(shells: State<'_, ssh::Shells>, sid: String) -> Result<ssh::Probe, String> {
    let h = shells.0.lock().await.get(&sid).ok_or("Shell geschlossen")?.handle();
    ssh::probe(&h).await
}

#[tauri::command]
async fn shell_resize(shells: State<'_, ssh::Shells>, sid: String, cols: u32, rows: u32) -> Result<(), String> {
    if let Some(t) = local_term(&sid) {
        return t.resize(cols.min(u16::MAX as u32) as u16, rows.min(u16::MAX as u32) as u16);
    }
    shells.0.lock().await.get(&sid).ok_or("Shell geschlossen")?.resize(cols, rows).await
}

#[tauri::command]
async fn shell_close(shells: State<'_, ssh::Shells>, sid: String) -> Result<(), String> {
    relay::SHELLS.lock().unwrap().remove(&sid);
    for d in DOORS.lock().unwrap().remove(&sid).unwrap_or_default() {
        d.3.abort();
    }
    if let Some(t) = localterm::TERMS.lock().unwrap().remove(&sid) {
        t.close();
    }
    if let Some(s) = shells.0.lock().await.remove(&sid) {
        s.close().await;
    }
    Ok(())
}

// ---------- Dateien (SFTP, sonst SCP; oder FTP) ----------

#[derive(Clone)]
enum FileSession {
    Sftp(Arc<sftp::Sftp>),
    Scp(Arc<scp::Scp>),
    Ftp(Arc<ftp::Ftp>),
}

#[derive(Default)]
struct Files(Mutex<HashMap<String, FileSession>>);

#[derive(Default)]
struct Transfers(Arc<Mutex<HashMap<String, tauri::async_runtime::JoinHandle<()>>>>);

fn files_get(files: &Files, sid: &str) -> Result<FileSession, String> {
    files.0.lock().unwrap().get(sid).cloned().ok_or_else(|| "Dateisitzung geschlossen".into())
}

#[derive(serde::Serialize)]
struct FilesOpened {
    sid: String,
    /// `true` = Server ohne SFTP, Übertragung über SCP (eingeschränkt).
    scp: bool,
}

/// FTP-Sitzung öffnen; `trust` = bestätigter Zertifikat-Fingerabdruck, gemerkt unter "host:port" wie bei RDP/VNC.
async fn ftp_open(path: &LibraryPath, c: &Connection, trust: Option<String>, remember: bool) -> Result<ftp::Ftp, String> {
    let port = c.ftp_port.ok_or("FTP ist für diese Verbindung ausgeschaltet")?;
    let pins_path = path.0.with_file_name("known_certs.json");
    let mut pins = load_pins(&pins_path);
    let pin_key = format!("{}:{port}", c.address);
    let password = login_secret(c, Kind::Password)?.unwrap_or_default();
    diag::add_secret(&password);
    let t = ftp::Target {
        host: c.address.clone(),
        port,
        user: c.user.clone(),
        password,
        security: ftp::Security::parse(&c.ftp_security)?,
        active: c.ftp_active,
        cert: trust.clone().or_else(|| pins.get(&pin_key).cloned()),
        skip_cert_check: c.skip_cert_check,
    };
    let (s, seen) = ftp::Ftp::open(t).await?;
    if let Some(fp) = trust.filter(|fp| remember && seen.as_ref() == Some(fp)) {
        pins.insert(pin_key, fp);
        let _ = std::fs::write(&pins_path, serde_json::to_string_pretty(&pins).unwrap_or_default());
    }
    Ok(s)
}

/// `trust`: bei SSH der Host-Schlüssel, bei FTPS das Zertifikat (`remember` = merken).
#[tauri::command]
async fn sftp_open(path: State<'_, LibraryPath>, files: State<'_, Files>, id: String, trust: Option<String>, remember: Option<bool>) -> Result<FilesOpened, String> {
    let c = find(&path, &id)?;
    if c.files_over_ftp() {
        let s = ftp_open(&path, &c, trust, remember.unwrap_or(true)).await?;
        let sid = new_sid(&c.id);
        files.0.lock().unwrap().insert(sid.clone(), FileSession::Ftp(Arc::new(s)));
        return Ok(FilesOpened { sid, scp: false });
    }
    let h = ssh::connect(&ssh_target(&c, trust)?).await?;
    let session = match c.file_transfer.as_str() {
        "scp" => FileSession::Scp(Arc::new(scp::Scp::new(h))),
        mode => match sftp::start(h).await {
            Ok(s) => FileSession::Sftp(Arc::new(s)),
            Err((_, e)) if mode == "sftp" => return Err(e),
            Err((h, e)) => {
                tracing::info!("SFTP nicht verfügbar ({e}), weiter mit SCP");
                FileSession::Scp(Arc::new(scp::Scp::new(h)))
            }
        },
    };
    let sid = new_sid(&c.id);
    touch(&path, &c.id);
    let scp = matches!(session, FileSession::Scp(_));
    files.0.lock().unwrap().insert(sid.clone(), session);
    Ok(FilesOpened { sid, scp })
}

#[tauri::command]
async fn sftp_list(files: State<'_, Files>, sid: String, path: String) -> Result<sftp::Listing, String> {
    match files_get(&files, &sid)? {
        FileSession::Sftp(s) => s.list(&path).await,
        FileSession::Scp(s) => s.list(&path).await,
        FileSession::Ftp(s) => s.list(&path).await,
    }
}

#[tauri::command]
async fn sftp_mkdir(files: State<'_, Files>, sid: String, dir: String, name: String) -> Result<String, String> {
    match files_get(&files, &sid)? {
        FileSession::Sftp(s) => s.mkdir(&dir, &name).await,
        FileSession::Scp(s) => s.mkdir(&dir, &name).await,
        FileSession::Ftp(s) => s.mkdir(&dir, &name).await,
    }
}

#[tauri::command]
async fn sftp_rename(files: State<'_, Files>, sid: String, path: String, name: String) -> Result<String, String> {
    match files_get(&files, &sid)? {
        FileSession::Sftp(s) => s.rename(&path, &name).await,
        FileSession::Ftp(s) => s.rename(&path, &name).await,
        FileSession::Scp(_) => Err(scp::LIMITED.into()),
    }
}

#[tauri::command]
async fn sftp_remove(files: State<'_, Files>, sid: String, path: String) -> Result<(), String> {
    match files_get(&files, &sid)? {
        FileSession::Sftp(s) => s.remove(&path).await,
        FileSession::Ftp(s) => s.remove(&path).await,
        FileSession::Scp(_) => Err(scp::LIMITED.into()),
    }
}

#[tauri::command]
async fn sftp_chmod(files: State<'_, Files>, sid: String, path: String, mode: u32) -> Result<(), String> {
    match files_get(&files, &sid)? {
        FileSession::Sftp(s) => s.chmod(&path, mode).await,
        FileSession::Ftp(s) => s.chmod(&path, mode).await,
        FileSession::Scp(_) => Err(scp::LIMITED.into()),
    }
}

fn ssh_files(files: &Files, sid: &str) -> Result<Arc<sftp::Sftp>, String> {
    match files_get(files, sid)? {
        FileSession::Sftp(s) => Ok(s),
        _ => Err("Nur bei Dateien über SSH möglich".into()),
    }
}

/// Besitzer und Gruppe («benutzer:gruppe») für den Rechte-Dialog; Linux `stat -c`, BSD/macOS `stat -f`.
#[tauri::command]
async fn sftp_owner(files: State<'_, Files>, sid: String, path: String) -> Result<String, String> {
    let q = scp::quote(&path)?;
    ssh_files(&files, &sid)?.exec(&format!("stat -c '%U:%G' -- {q} 2>/dev/null || stat -f '%Su:%Sg' -- {q}"), false, None).await
}

/// Rechte und Besitzer setzen, auf Wunsch rekursiv und mit sudo (nur nach Bestätigung in der Oberfläche).
#[tauri::command]
#[allow(clippy::too_many_arguments)]
async fn sftp_set_perms(files: State<'_, Files>, sid: String, path: String, mode: Option<u32>, owner: Option<String>, recursive: bool, sudo: bool, password: Option<String>) -> Result<(), String> {
    let script = sftp::perms_script(&path, mode, owner.as_deref(), recursive)?;
    ssh_files(&files, &sid)?.exec(&script, sudo, password.as_deref()).await.map(|_| ())
}

/// Temporärer Ordner auf dem Server (für Hochladen mit Administratorrechten).
#[tauri::command]
async fn sftp_tempdir(files: State<'_, Files>, sid: String) -> Result<String, String> {
    ssh_files(&files, &sid)?.exec("mktemp -d", false, None).await
}

/// Hochgeladenes aus dem temporären Ordner mit sudo an den Zielort verschieben (nie überschreiben).
#[tauri::command]
async fn sftp_sudo_move(files: State<'_, Files>, sid: String, from: String, dest_dir: String, replace: bool, password: Option<String>) -> Result<(), String> {
    let script = sftp::move_script(&from, &dest_dir, replace)?;
    ssh_files(&files, &sid)?.exec(&script, true, password.as_deref()).await.map(|_| ())
}

/// In den Papierkorb auf dem Server statt löschen; gibt den Pfad im Papierkorb zurück.
#[tauri::command]
async fn sftp_trash(files: State<'_, Files>, sid: String, path: String) -> Result<String, String> {
    ssh_files(&files, &sid)?.to_trash(&path).await
}

/// Zeitstempel-Ordner im Papierkorb anlegen (für die Erhöhung mit sudo).
#[tauri::command]
async fn sftp_trash_slot(files: State<'_, Files>, sid: String) -> Result<String, String> {
    ssh_files(&files, &sid)?.trash_slot().await
}

/// Grösse (für das Grössenlimit des Papierkorbs).
#[tauri::command]
async fn sftp_size(files: State<'_, Files>, sid: String, path: String) -> Result<u64, String> {
    ssh_files(&files, &sid)?.size(&path).await
}

/// Rückgängig: aus dem Papierkorb an den alten Ort.
#[tauri::command]
async fn sftp_restore(files: State<'_, Files>, sid: String, trashed: String, original: String) -> Result<(), String> {
    ssh_files(&files, &sid)?.restore(&trashed, &original).await
}

/// Papierkorb aufräumen (älter als `days` Tage).
#[tauri::command]
async fn sftp_trash_cleanup(files: State<'_, Files>, sid: String, days: u32) -> Result<usize, String> {
    ssh_files(&files, &sid)?.trash_cleanup(days).await
}

/// Löschen, Umbenennen, Ordner/Datei anlegen oder zum Herunterladen kopieren, mit sudo (nur nach Bestätigung).
#[tauri::command]
async fn sftp_sudo_op(files: State<'_, Files>, sid: String, op: String, path: String, arg: String, password: Option<String>) -> Result<(), String> {
    let script = sftp::sudo_op_script(&op, &path, &arg)?;
    ssh_files(&files, &sid)?.exec(&script, true, password.as_deref()).await.map(|_| ())
}

#[tauri::command]
async fn sftp_touch(files: State<'_, Files>, sid: String, dir: String, name: String) -> Result<String, String> {
    match files_get(&files, &sid)? {
        FileSession::Sftp(s) => s.touch(&dir, &name).await,
        FileSession::Ftp(s) => s.touch(&dir, &name).await,
        FileSession::Scp(_) => Err(scp::LIMITED.into()),
    }
}

/// Prüfsumme einer Datei auf dem Server (für «Inhalt prüfen» im Vergleich).
#[tauri::command]
async fn sftp_sha256(files: State<'_, Files>, sid: String, path: String) -> Result<String, String> {
    match files_get(&files, &sid)? {
        FileSession::Sftp(s) => s.sha256(&path).await,
        FileSession::Scp(s) => s.sha256(&path).await,
        FileSession::Ftp(_) => Err("Inhalt prüfen geht bei FTP nicht (keine Prüfsummen auf dem Server); verglichen wird nach Grösse und Datum.".into()),
    }
}

#[tauri::command]
async fn local_sha256(path: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || sftp::local_sha256(&path)).await.map_err(|e| e.to_string())?
}

#[tauri::command]
fn local_mkdir(dir: String, name: String) -> Result<String, String> {
    sftp::local_mkdir(&dir, &name)
}

#[tauri::command]
fn local_rename(path: String, name: String) -> Result<String, String> {
    sftp::local_rename(&path, &name)
}

/// Profile des lokalen Terminals (installierte Shells und Interpreter).
#[tauri::command]
async fn term_profiles() -> Vec<profiles::Profile> {
    tauri::async_runtime::spawn_blocking(profiles::detect).await.unwrap_or_default()
}

/// Laufende SSH-Tunnel je Desktop-Sitzung (beim Trennen beendet).
type Tunnel = (Arc<russh::client::Handle<ssh::Client>>, tokio::task::JoinHandle<()>);
/// Beendet den Tunnel, wenn er nicht an eine Sitzung übergeben wurde (Fehler beim Verbinden).
struct TunnelGuard(Option<Tunnel>);
impl Drop for TunnelGuard {
    fn drop(&mut self) {
        if let Some((_, task)) = self.0.take() {
            task.abort();
        }
    }
}
static TUNNELS: std::sync::LazyLock<Mutex<HashMap<String, Tunnel>>> = std::sync::LazyLock::new(Default::default);
fn tunnel_end(sid: &str) {
    if let Some((_, task)) = TUNNELS.lock().unwrap().remove(sid) {
        task.abort();
    }
}

/// «Sicher über SSH verbinden»: SSH zum Rechner, lokaler Port → `port` dort. Gibt (Host, Port) für RDP/VNC zurück.
async fn desktop_via_ssh(c: &Connection, port: u16) -> Result<(String, u16, Option<Tunnel>), String> {
    if !c.ssh_tunnel {
        return Ok((c.address.clone(), port, None));
    }
    if c.ssh_port.is_none() {
        return Err("«Sicher über SSH verbinden» braucht SSH bei dieser Verbindung (Dienste › Shell).".into());
    }
    let t = ssh_target(c, None)?;
    let h = ssh::connect(&t).await.map_err(|e| {
        if e.starts_with(ssh::HOSTKEY_UNKNOWN) {
            "SSH-Tunnel: Der Schlüssel des Servers ist noch nicht bestätigt. Bitte einmal die Shell öffnen und ihn bestätigen.".to_string()
        } else {
            format!("SSH-Tunnel: {e}")
        }
    })?;
    let h = Arc::new(h);
    let (local, task) = forward::tunnel(h.clone(), port).await?;
    tracing::info!(port, local, "SSH-Tunnel für den Desktop steht");
    Ok(("127.0.0.1".into(), local, Some((h, task))))
}

/// Erreichbarkeit einer Verbindung (Latenz in ms). Merkt sich dabei die MAC-Adresse für Wake-on-LAN.
#[tauri::command]
async fn conn_reach(path: State<'_, LibraryPath>, id: String) -> Result<Option<u32>, String> {
    let c = find(&path, &id)?;
    let ports: Vec<u16> = [c.rdp_port, c.vnc_port, c.ssh_port, c.ftp_port].into_iter().flatten().collect();
    let ms = wake::reach(&c.address, &ports).await;
    if ms.is_some() && c.mac.is_empty() {
        let host = c.address.clone();
        if let Some(mac) = tauri::async_runtime::spawn_blocking(move || wake::mac_of(&host)).await.ok().flatten() {
            let _g = LIBRARY_LOCK.lock().unwrap();
            if let Ok(mut items) = library::load(&path.0) {
                if let Some(x) = items.iter_mut().find(|x| x.id == id) {
                    x.mac = mac;
                    let _ = library::save(&path.0, &items);
                }
            }
        }
    }
    Ok(ms)
}

/// Rechner per Wake-on-LAN aufwecken (MAC aus der Verbindung).
#[tauri::command]
fn conn_wake(path: State<'_, LibraryPath>, id: String) -> Result<(), String> {
    let c = find(&path, &id)?;
    if c.mac.is_empty() {
        return Err("Für diesen Rechner ist keine MAC-Adresse bekannt. Sesam merkt sie sich, sobald er einmal eingeschaltet erreichbar war; sie lässt sich auch unter «Erweitert» eintragen.".into());
    }
    wake::wake(&c.mac)
}

/// Selbsttest der Windows-Tastenkombinationen (Befehlspalette «Diagnose: Tastentest»), läuft ausserhalb des Hauptthreads.
#[tauri::command]
async fn kbhook_selftest() -> String {
    tauri::async_runtime::spawn_blocking(kbhook::selftest).await.unwrap_or_default()
}

/// Windows-Tastenkombinationen an den entfernten Rechner geben (Desktop-Tab mit Fokus), siehe kbhook.
#[tauri::command]
fn kbhook_set(active: bool) -> usize {
    kbhook::set_active(active);
    kbhook::calls()
}

/// Text aus der Zwischenablage des Systems (ohne Berechtigungsabfrage der Webansicht).
#[tauri::command]
fn clip_read() -> Result<String, String> {
    clipshare::get_text()
}

/// Text in die Zwischenablage des Systems.
#[tauri::command]
fn clip_write(text: String) -> Result<(), String> {
    clipshare::set_text(&text)
}

/// «Beenden» aus dem eigenen Menü (Windows/Linux).
#[tauri::command]
fn app_exit(app: AppHandle) {
    app.exit(0);
}

/// Laufende Desktop-Sitzung (VNC/RDP) an das Fenster dieses Kanals hängen.
#[tauri::command]
fn desktop_attach(sid: String, on_event: Channel<InvokeResponseBody>) -> Result<(), String> {
    relay::DESKTOPS.lock().unwrap().get(&sid).ok_or("Desktop-Sitzung geschlossen")?.swap(on_event);
    Ok(())
}

/// Laufende Shell an das Fenster dieses Kanals hängen.
#[tauri::command]
fn shell_attach(sid: String, on_event: Channel<ssh::ShellEvent>) -> Result<(), String> {
    relay::SHELLS.lock().unwrap().get(&sid).ok_or("Shell geschlossen")?.swap(on_event);
    Ok(())
}

/// Sitzung in ein eigenes Fenster abkoppeln; `token` verweist auf die Übergabe im localStorage.
/// Einstellungen als eigenes Fenster (macOS, HIG): vorhandenes nach vorne holen und den Bereich wechseln, sonst öffnen.
#[tauri::command]
async fn open_settings_window(app: AppHandle, tab: String) -> Result<(), String> {
    if !tab.chars().all(|c| c.is_ascii_alphanumeric()) {
        return Err("Unbekannter Bereich".into());
    }
    if let Some(w) = app.get_webview_window("settings") {
        let _ = w.show();
        let _ = w.set_focus();
        return w.emit("settings-tab", tab).map_err(|e| e.to_string());
    }
    tauri::WebviewWindowBuilder::new(&app, "settings", tauri::WebviewUrl::App("index.html".into()))
        .initialization_script(format!("window.__SESAM_SETTINGS__ = '{tab}';"))
        .title("Einstellungen")
        .inner_size(820.0, 600.0)
        .min_inner_size(640.0, 440.0)
        .build()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// Asynchron: ein Fenster in einem synchronen Befehl zu erzeugen, blockiert unter Windows (leeres Fenster, X reagiert nicht).
#[tauri::command]
async fn open_detached(app: AppHandle, token: String, title: String) -> Result<(), String> {
    if !token.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        return Err("Ungültige Übergabe".into());
    }
    // iPad: keine weiteren Fenster (das Frontend bietet Abkoppeln dort nicht an).
    #[cfg(mobile)]
    return {
        let _ = (app, title);
        Err("Eigene Fenster gibt es auf diesem Gerät nicht".into())
    };
    #[cfg(desktop)]
    let builder = tauri::WebviewWindowBuilder::new(&app, format!("detached-{token}"), tauri::WebviewUrl::App("index.html".into()))
        .initialization_script(format!("window.__SESAM_DETACH__ = '{token}';"))
        .title(title)
        .inner_size(1200.0, 800.0)
        .min_inner_size(480.0, 320.0)
        .decorations(cfg!(target_os = "macos"))
        .shadow(true);
    #[cfg(target_os = "macos")]
    let builder = builder.title_bar_style(tauri::TitleBarStyle::Overlay).hidden_title(true);
    #[cfg(desktop)]
    {
        builder.build().map_err(|e| e.to_string())?;
        Ok(())
    }
}

#[tauri::command]
fn sftp_close(files: State<Files>, sid: String) {
    files.0.lock().unwrap().remove(&sid);
}

/// Setup-Script für GNOME Remote Desktop, unverändert eingebettet (LF, ohne BOM).
const SETUP_SCRIPT: &[u8] = include_bytes!("../resources/sesam-rdp-setup.sh");

/// Script-Inhalt und SHA-256 (zum Herunterladen und als Versionsangabe).
#[tauri::command]
fn setup_script() -> (Vec<u8>, String) {
    use sha2::Digest;
    (SETUP_SCRIPT.to_vec(), sha2::Sha256::digest(SETUP_SCRIPT).iter().map(|b| format!("{b:02x}")).collect())
}

/// Script per SFTP nach `~/sesam-rdp-setup.sh` hochladen (Bytes 1:1). Ausführen tut der Benutzer selbst in der Shell.
#[tauri::command]
async fn setup_upload(path: State<'_, LibraryPath>, id: String, trust: Option<String>) -> Result<String, String> {
    use tokio::io::AsyncWriteExt;
    let c = find(&path, &id)?;
    let s = sftp::open(ssh_target(&c, trust)?).await?;
    let home = s.s.canonicalize(".").await.map_err(|e| e.to_string())?;
    let target = sftp::rjoin(&home, "sesam-rdp-setup.sh");
    let mut f = s.s.create(&target).await.map_err(|e| format!("{target}: {e}"))?;
    f.write_all(SETUP_SCRIPT).await.map_err(|e| e.to_string())?;
    f.shutdown().await.map_err(|e| e.to_string())?;
    Ok(target)
}

fn thumb_path(path: &LibraryPath, id: &str) -> Result<PathBuf, String> {
    if !id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') || id.is_empty() {
        return Err("Ungültige ID".into());
    }
    Ok(path.0.with_file_name("thumbs").join(format!("{id}.jpg")))
}

/// Vorschaubild speichern (JPEG vom Frontend: letzter Desktop beim Trennen).
#[tauri::command]
fn thumb_save(path: State<LibraryPath>, id: String, data: Vec<u8>) -> Result<(), String> {
    if id.starts_with("quick-") {
        return Ok(()); // schnelle Verbindungen hinterlassen nichts
    }
    let p = thumb_path(&path, &id)?;
    std::fs::create_dir_all(p.parent().unwrap()).map_err(|e| e.to_string())?;
    std::fs::write(&p, data).map_err(|e| e.to_string())
}

#[tauri::command]
fn thumb_get(path: State<LibraryPath>, id: String) -> Option<Vec<u8>> {
    std::fs::read(thumb_path(&path, &id).ok()?).ok()
}

/// Eigenes Bild übernehmen (vom Benutzer im Dialog gewählt).
#[tauri::command]
fn thumb_import(path: State<LibraryPath>, id: String, file: String) -> Result<(), String> {
    let data = std::fs::read(&file).map_err(|e| format!("{file}: {e}"))?;
    if data.len() > 8 << 20 {
        return Err("Bild zu gross (höchstens 8 MB)".into());
    }
    let p = thumb_path(&path, &id)?;
    std::fs::create_dir_all(p.parent().unwrap()).map_err(|e| e.to_string())?;
    std::fs::write(&p, data).map_err(|e| e.to_string())
}

/// Verknüpfung auf dem Schreibtisch: öffnet `sesam://open?id=…&view=…`.
/// Windows: .url mit dem Sesam-Symbol, macOS: .webloc.
#[tauri::command]
fn create_shortcut(app: AppHandle, path: State<LibraryPath>, id: String, view: String) -> Result<String, String> {
    let c = find(&path, &id)?;
    if !["desktop", "shell", "files"].contains(&view.as_str()) {
        return Err("Unbekannte Ansicht".into());
    }
    let label = match view.as_str() { "desktop" => "Desktop", "shell" => "Shell", _ => "Dateien" };
    let safe: String = c.name.chars().map(|ch| if r#"\/:*?"<>|"#.contains(ch) || ch.is_control() { '_' } else { ch }).collect();
    let url = format!("sesam://open?id={}&view={view}", c.id);
    let desk = app.path().desktop_dir().map_err(|e| e.to_string())?;
    #[cfg(target_os = "macos")]
    let (file, body) = (
        desk.join(format!("{safe} ({label}).webloc")),
        format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n<plist version=\"1.0\"><dict><key>URL</key><string>{url}</string></dict></plist>\n"),
    );
    #[cfg(not(target_os = "macos"))]
    let (file, body) = {
        let exe = std::env::current_exe().map(|p| p.display().to_string()).unwrap_or_default();
        (desk.join(format!("{safe} ({label}).url")), format!("[InternetShortcut]\r\nURL={url}\r\nIconFile={exe}\r\nIconIndex=0\r\n"))
    };
    std::fs::write(&file, body).map_err(|e| format!("{}: {e}", file.display()))?;
    #[cfg(target_os = "macos")]
    sfsymbol::set_app_icon(&file);
    Ok(file.display().to_string())
}

/// Dateien aufs Desktop-Fenster gezogen: per SFTP nach ~/Sesam-Ablage (wenn SSH an ist),
/// sonst in den per RDP freigegebenen Ordner kopieren. Gibt eine Meldung für die Statuszeile zurück.
#[tauri::command]
async fn drop_files(path: State<'_, LibraryPath>, id: String, files: Vec<String>) -> Result<String, String> {
    let c = find(&path, &id)?;
    let files: Vec<PathBuf> = files.into_iter().map(PathBuf::from).collect();
    if c.ssh_port.is_some() {
        let share = ClipShare::new(c.clone(), std::env::temp_dir());
        let done = share.upload_files(&files).await?;
        return Ok(match done.as_slice() {
            [one] => format!("Hochgeladen nach {one}"),
            _ => format!("{} Elemente hochgeladen nach {}", done.len(), done.first().map(|p| sftp::rparent(p).unwrap_or_default()).unwrap_or_default()),
        });
    }
    let dir = c.shared_folder.trim();
    if c.rdp_port.is_some() && !dir.is_empty() {
        let dir = PathBuf::from(dir);
        for f in &files {
            let name = f.file_name().ok_or("Ungültige Datei")?;
            let mut target = dir.join(name);
            let mut n = 2;
            while target.exists() {
                target = dir.join(format!("{} ({n})", name.to_string_lossy()));
                n += 1;
            }
            if f.is_dir() {
                return Err("Ordner lassen sich nur mit SSH übertragen.".into());
            }
            std::fs::copy(f, &target).map_err(|e| format!("{}: {e}", f.display()))?;
        }
        return Ok(format!("{} Datei(en) in den freigegebenen Ordner kopiert (auf dem Server als Laufwerk sichtbar).", files.len()));
    }
    Err(format!("«{}» hat kein SSH; darum kann Sesam keine Dateien hinüberlegen. SSH bei der Verbindung einschalten (Bearbeiten › Dienste) oder unter RDP einen Ordner freigeben.", c.name))
}

/// Gemerkte TLS-Zertifikate (RDP/VNC) für die Einstellungen.
#[tauri::command]
fn certs_list(path: State<LibraryPath>) -> Vec<(String, String)> {
    let mut v: Vec<_> = load_pins(&path.0.with_file_name("known_certs.json")).into_iter().collect();
    v.sort();
    v
}

#[tauri::command]
fn cert_remove(path: State<LibraryPath>, host: String) -> Result<(), String> {
    let p = path.0.with_file_name("known_certs.json");
    let mut pins = load_pins(&p);
    pins.remove(&host);
    std::fs::write(&p, serde_json::to_string_pretty(&pins).unwrap_or_default()).map_err(|e| e.to_string())
}

/// Vom Benutzer im Speichern-Dialog gewählte Datei schreiben (z. B. Bildschirmfoto).
#[tauri::command]
fn save_bytes(path: String, data: Vec<u8>) -> Result<(), String> {
    std::fs::write(&path, data).map_err(|e| format!("{path}: {e}"))
}

#[tauri::command]
fn local_list(app: AppHandle, path: String) -> Result<sftp::Listing, String> {
    // iPad: nur der Ordner «Documents» der App (in der Dateien-App «Auf meinem iPad › Sesam Remote»), darüber nichts.
    #[cfg(mobile)]
    {
        let root = local_home(app)?;
        let inside = std::path::Path::new(&path).canonicalize().ok().zip(std::path::Path::new(&root).canonicalize().ok()).is_some_and(|(p, r)| p.starts_with(r));
        let mut l = sftp::local_list(if inside { &path } else { &root })?;
        if !inside || l.path.trim_end_matches('/') == root.trim_end_matches('/') {
            l.parent = None;
        }
        return Ok(l);
    }
    #[cfg(desktop)]
    {
        let _ = app;
        sftp::local_list(&path)
    }
}

#[tauri::command]
fn local_home(app: AppHandle) -> Result<String, String> {
    #[cfg(mobile)]
    {
        let d = app.path().document_dir().map_err(|e| e.to_string())?;
        std::fs::create_dir_all(&d).map_err(|e| e.to_string())?;
        return Ok(d.to_string_lossy().into_owned());
    }
    #[cfg(desktop)]
    app.path().home_dir().map(|p| p.to_string_lossy().into_owned()).map_err(|e| e.to_string())
}

/// Startet eine Übertragung im Hintergrund; Fortschritt über `on_progress`. Gibt die Übertragungs-ID zurück.
#[tauri::command]
fn transfer_start(
    files: State<Files>,
    transfers: State<Transfers>,
    sid: String,
    upload: bool,
    src: String,
    dest_dir: String,
    replace: Option<bool>,
    on_progress: Channel<sftp::Progress>,
) -> Result<String, String> {
    let s = files_get(&files, &sid)?;
    let tid = new_sid("t");
    let (map, key) = (transfers.0.clone(), tid.clone());
    let mut running = transfers.0.lock().unwrap();
    let task = tauri::async_runtime::spawn(async move {
        let ch = on_progress.clone();
        let report = move |p| {
            let _ = ch.send(p);
        };
        let (local, remote) = if upload { (&src, &dest_dir) } else { (&dest_dir, &src) };
        let local = std::path::Path::new(local);
        let replace = replace.unwrap_or(false);
        let r = match (&s, upload) {
            (FileSession::Sftp(s), true) => sftp::upload(s, local, remote, replace, report).await,
            (FileSession::Sftp(s), false) => sftp::download(s, remote, local, replace, report).await,
            (FileSession::Scp(s), true) => scp::upload(s, local, remote, replace, report).await,
            (FileSession::Scp(s), false) => scp::download(s, remote, local, replace, report).await,
            // FTP: «Ersetzen» gibt es nicht, es bleibt bei «Name (2)».
            (FileSession::Ftp(_), _) if replace => Err("Ersetzen ist bei FTP nicht möglich; die Datei wird als «Name (2)» abgelegt, wenn du ohne Ersetzen überträgst.".into()),
            (FileSession::Ftp(s), true) => ftp::upload(s, local, remote, report).await,
            (FileSession::Ftp(s), false) => ftp::download(s, remote, local, report).await,
        };
        if let Err(error) = r {
            let _ = on_progress.send(sftp::Progress::Failed { error });
        }
        map.lock().unwrap().remove(&key);
    });
    running.insert(tid.clone(), task);
    Ok(tid)
}

#[tauri::command]
fn transfer_cancel(transfers: State<Transfers>, tid: String) {
    if let Some(t) = transfers.0.lock().unwrap().remove(&tid) {
        t.abort();
    }
}

// ---------- VNC ----------

struct Desktop {
    session: Arc<vnc::Session>,
    share: Arc<ClipShare>,
    events: relay::Relay<InvokeResponseBody>,
}

#[derive(Default)]
struct Desktops(Mutex<HashMap<String, Desktop>>);

/// Gepinnte TLS-Zertifikate der VNC-Server: "host:port" → SHA256.
fn load_pins(path: &std::path::Path) -> HashMap<String, String> {
    std::fs::read_to_string(path).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
}

/// Binärrahmen ans Frontend (Big Endian): 0 Rechteck, 1 Grösse, 3 Ende, 4 Glocke, 5 Hinweis, 6 Mauszeiger.
/// Ein Bild mit allen geänderten Rechtecken (Art 7): Anzahl, dann je x, y, b, h und RGBA.
fn frame_rects(list: &[(vnc::decode::Rect, Vec<u8>)]) -> InvokeResponseBody {
    let mut b = Vec::with_capacity(3 + list.iter().map(|(_, p)| 8 + p.len()).sum::<usize>());
    b.push(7);
    b.extend((list.len() as u16).to_be_bytes());
    for (r, px) in list {
        [r.x, r.y, r.w, r.h].iter().for_each(|n| b.extend(n.to_be_bytes()));
        b.extend_from_slice(px);
    }
    InvokeResponseBody::Raw(b)
}

fn frame(kind: u8, nums: &[u16], tail: &[u8]) -> InvokeResponseBody {
    let mut b = Vec::with_capacity(1 + nums.len() * 2 + tail.len());
    b.push(kind);
    nums.iter().for_each(|n| b.extend(n.to_be_bytes()));
    b.extend_from_slice(tail);
    InvokeResponseBody::Raw(b)
}

#[tauri::command]
async fn vnc_open(
    app: AppHandle,
    path: State<'_, LibraryPath>,
    desktops: State<'_, Desktops>,
    id: String,
    trust: Option<String>,
    remember: Option<bool>,
    on_event: Channel<InvokeResponseBody>,
) -> Result<String, String> {
    let c = find(&path, &id)?;
    let port = c.vnc_port.ok_or("Desktop (VNC) ist für diese Verbindung ausgeschaltet")?;
    let password = login_secret(&c, Kind::Password)?;
    let vnc_password = match &c.vnc_credential_id {
        Some(cid) => cred_login(&path, cid)?.1,
        None => vault::get(&c.id, Kind::VncPassword)?,
    };
    let pins_path = path.0.with_file_name("known_certs.json");
    let mut pins = load_pins(&pins_path);
    let pin_key = format!("{}:{port}", c.address);
    let cert = if c.skip_cert_check {
        vnc::tls::Pinned::accept_any()
    } else {
        vnc::tls::Pinned::new(trust.clone().or_else(|| pins.get(&pin_key).cloned()))
    };
    let login = vnc::Login { host: &c.address, user: &c.user, password: password.as_deref(), vnc_password: vnc_password.as_deref(), cert };
    let timeout = std::time::Duration::from_secs(15);
    let (via_host, via_port, tunnel) = desktop_via_ssh(&c, port).await?;
    let mut tunnel = TunnelGuard(tunnel);
    let stream = tokio::time::timeout(timeout, tokio::net::TcpStream::connect((via_host.as_str(), via_port)))
        .await
        .map_err(|_| "Zeitüberschreitung beim Verbinden".to_string())?
        .map_err(|e| format!("Verbindung fehlgeschlagen: {e}"))?;
    let _ = stream.set_nodelay(true);
    let (stream, init) = tokio::time::timeout(timeout, vnc::handshake(stream, &login))
        .await
        .map_err(|_| "Zeitüberschreitung bei der Anmeldung".to_string())??;
    // Bestätigtes Zertifikat merken, aber nur wenn TLS es wirklich gesehen hat.
    if let Some(fp) = trust.filter(|fp| remember.unwrap_or(true) && login.cert.seen.lock().unwrap().as_ref() == Some(fp)) {
        pins.insert(pin_key, fp);
        let _ = std::fs::write(&pins_path, serde_json::to_string_pretty(&pins).unwrap_or_default());
    }

    let downloads = app.path().download_dir().unwrap_or_else(|_| std::env::temp_dir());
    let share = Arc::new(ClipShare::new(c.clone(), downloads));
    let on_event = relay::Relay::new(on_event);
    let (sh, events) = (share.clone(), on_event.clone());
    let session = vnc::Session::start(stream, init, move |e| {
        let f = match e {
            vnc::Event::Frame(list) => frame_rects(&list),
            vnc::Event::Resize { width, height } => frame(1, &[width, height], &[]),
            vnc::Event::Closed(why) => frame(3, &[], why.as_bytes()),
            vnc::Event::Bell => frame(4, &[], &[]),
            vnc::Event::Info(t) => frame(5, &[], t.as_bytes()),
            vnc::Event::Cursor { hot_x, hot_y, w, h, rgba } => frame(6, &[hot_x, hot_y, w, h], &rgba),
            vnc::Event::Clipboard(t) => {
                if !sh.enabled {
                    return true;
                }
                if let Some(paths) = clipshare::file_uris(&t).filter(|_| sh.has_ssh()) {
                    let (sh, ev) = (sh.clone(), on_event.clone());
                    tauri::async_runtime::spawn(async move {
                        let msg = match sh.download_to_clipboard(&paths).await {
                            Ok(n) => format!("{n} Datei(en) vom Server liegen in der Zwischenablage ({})", sh.download_dir().join("Sesam-Ablage").display()),
                            Err(e) => e,
                        };
                        let _ = ev.send(frame(5, &[], msg.as_bytes()));
                    });
                    return true;
                }
                clipshare::write_local_text(&t);
                *sh.last.lock().unwrap() = Last::Text(t);
                return true;
            }
        };
        on_event.send(f).is_ok()
    });
    let sid = new_sid(&c.id);
    touch(&path, &c.id);
    relay::DESKTOPS.lock().unwrap().insert(sid.clone(), events.clone());
    desktops.0.lock().unwrap().insert(sid.clone(), Desktop { session: Arc::new(session), share, events });
    if let Some(t) = tunnel.0.take() {
        TUNNELS.lock().unwrap().insert(sid.clone(), t);
    }
    Ok(sid)
}

fn with_desktop<T>(d: &Desktops, sid: &str, f: impl FnOnce(&Desktop) -> T) -> Result<T, String> {
    d.0.lock().unwrap().get(sid).map(f).ok_or_else(|| "Desktop-Sitzung geschlossen".into())
}

/// Textdatei an einen im Speichern-Dialog gewählten Ort schreiben (z. B. exportiertes Terminal-Design).
#[tauri::command]
fn save_text_file(path: String, text: String) -> Result<(), String> {
    std::fs::write(&path, text).map_err(|e| format!("{path}: {e}"))
}

/// Freigaben des Betriebssystems mit Status (Einstellungen › «Berechtigungen einrichten …»).
#[tauri::command]
fn perm_list() -> Vec<perms::Perm> {
    perms::list()
}

/// Freigabe anfragen (Systemdialog) und den neuen Stand liefern.
#[tauri::command]
fn perm_request(id: String) -> Vec<perms::Perm> {
    perms::request(&id);
    perms::list()
}

/// Passende Stelle in den Systemeinstellungen öffnen.
#[tauri::command]
fn perm_open(id: String) -> Result<(), String> {
    perms::open(&id)
}

/// Die Oberfläche hat ein Bild gezeichnet (VNC oder RDP): das nächste darf kommen.
#[tauri::command]
fn desktop_ack(desktops: State<Desktops>, sessions: State<RdpSessions>, sid: String) {
    if let Some(d) = desktops.0.lock().unwrap().get(&sid) {
        d.session.ack();
    } else if let Some(s) = sessions.0.lock().unwrap().get(&sid) {
        s.send(rdp::Input::Ack);
    }
}

#[tauri::command]
fn vnc_pointer(desktops: State<Desktops>, sid: String, mask: u8, x: u16, y: u16) -> Result<(), String> {
    with_desktop(&desktops, &sid, |d| d.session.send(vnc::pointer(mask, x, y)))
}

#[tauri::command]
fn vnc_key(desktops: State<Desktops>, sid: String, down: bool, keysym: u32) -> Result<(), String> {
    with_desktop(&desktops, &sid, |d| d.session.send(vnc::key(down, keysym)))
}

/// `false`, wenn der Server keine Grössenänderung kann.
#[tauri::command]
fn vnc_set_size(desktops: State<Desktops>, sid: String, width: u16, height: u16) -> Result<bool, String> {
    with_desktop(&desktops, &sid, |d| d.session.set_size(width, height))
}

/// Lokale Zwischenablage an den Server geben, falls sie sich geändert hat (beim Fokussieren aufgerufen).
/// Text direkt über VNC; Dateien und Bilder per SFTP nach `~/Sesam-Ablage`, dazu deren Pfade als Text.
#[tauri::command]
async fn vnc_clipboard_sync(desktops: State<'_, Desktops>, sid: String) -> Result<(), String> {
    let (session, share, events) = with_desktop(&desktops, &sid, |d| (d.session.clone(), d.share.clone(), d.events.clone()))?;
    if !share.enabled {
        return Ok(());
    }
    let info = |m: &str| {
        let _ = events.send(frame(5, &[], m.as_bytes()));
    };
    // `true`, wenn neu (und merkt es sich).
    let fresh = |next: Last| {
        let mut last = share.last.lock().unwrap();
        let new = *last != next;
        *last = next;
        new
    };
    let needs_ssh = "Dateien und Bilder über die Zwischenablage brauchen SSH (in der Verbindung einschalten).";
    match clipshare::read_local() {
        Local::Nothing => {}
        Local::Text(t) => {
            if fresh(Last::Text(t.clone())) {
                session.clipboard(&t);
            }
        }
        Local::Files(files) => {
            if !fresh(Last::Files(files.clone())) {
                return Ok(());
            }
            if !share.has_ssh() {
                info(needs_ssh);
                return Ok(());
            }
            info(&format!("Kopiere {} Element(e) nach ~/Sesam-Ablage …", files.len()));
            match share.upload_files(&files).await {
                Ok(paths) => {
                    session.clipboard(&paths.join("\n"));
                    info(&format!("{} Element(e) in ~/Sesam-Ablage; der Pfad liegt in der Zwischenablage des Servers.", paths.len()));
                }
                Err(e) => info(&e),
            }
        }
        Local::Image { width, height, rgba } => {
            if !fresh(Last::Image(clipshare::hash(&rgba))) {
                return Ok(());
            }
            if !share.has_ssh() {
                info(needs_ssh);
                return Ok(());
            }
            let png = clipshare::encode_png(width, height, &rgba)?;
            match share.upload_png(&png).await {
                Ok(path) => {
                    session.clipboard(&path);
                    info(&format!("Bild als {path} abgelegt; der Pfad liegt in der Zwischenablage des Servers."));
                }
                Err(e) => info(&e),
            }
        }
    }
    Ok(())
}

#[tauri::command]
fn vnc_close(desktops: State<Desktops>, sid: String) {
    tunnel_end(&sid);
    relay::DESKTOPS.lock().unwrap().remove(&sid);
    if let Some(d) = desktops.0.lock().unwrap().remove(&sid) {
        d.session.close();
    }
}

// ---------- RDP ----------

#[derive(Default)]
struct RdpSessions(Mutex<HashMap<String, rdp::Session>>);

// Das Windows-Zwischenablage-Backend braucht ein Fenster auf einem Thread mit Nachrichtenschleife:
// es entsteht und lebt auf dem Tauri-Hauptthread, je RDP-Sitzung eines.
#[cfg(windows)]
thread_local! {
    static WIN_CLIPS: std::cell::RefCell<HashMap<String, ironrdp_cliprdr_native::WinClipboard>> = Default::default();
}

#[cfg(windows)]
async fn rdp_clipboard(app: &AppHandle, key: String, tx: tokio::sync::mpsc::UnboundedSender<rdp::Input>) -> Option<rdp::ClipFactory> {
    let (otx, orx) = tokio::sync::oneshot::channel();
    app.run_on_main_thread(move || {
        let made = ironrdp_cliprdr_native::WinClipboard::with_file_transfer(rdp::ClipProxy(tx), true).ok().map(|c| {
            let f = c.backend_factory();
            WIN_CLIPS.with(|m| m.borrow_mut().insert(key, c));
            f
        });
        let _ = otx.send(made);
    })
    .ok()?;
    orx.await.ok().flatten().map(|f| Arc::new(Mutex::new(f)))
}

/// macOS und Linux: Text über die Zwischenablage des Systems (rdpclip).
#[cfg(not(windows))]
async fn rdp_clipboard(_: &AppHandle, _: String, tx: tokio::sync::mpsc::UnboundedSender<rdp::Input>) -> Option<rdp::ClipFactory> {
    Some(Arc::new(Mutex::new(Box::new(rdpclip::Factory(tx)))))
}

fn rdp_clipboard_drop(app: &AppHandle, key: String) {
    #[cfg(windows)]
    let _ = app.run_on_main_thread(move || {
        WIN_CLIPS.with(|m| m.borrow_mut().remove(&key));
    });
    #[cfg(not(windows))]
    let _ = (app, key);
}

fn screen_frame(e: vnc::Event) -> Option<InvokeResponseBody> {
    Some(match e {
        vnc::Event::Frame(list) => frame_rects(&list),
        vnc::Event::Resize { width, height } => frame(1, &[width, height], &[]),
        vnc::Event::Closed(why) => frame(3, &[], why.as_bytes()),
        vnc::Event::Bell => frame(4, &[], &[]),
        vnc::Event::Info(t) => frame(5, &[], t.as_bytes()),
        vnc::Event::Cursor { hot_x, hot_y, w, h, rgba } => frame(6, &[hot_x, hot_y, w, h], &rgba),
        vnc::Event::Clipboard(_) => return None,
    })
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
async fn rdp_open(
    app: AppHandle,
    path: State<'_, LibraryPath>,
    sessions: State<'_, RdpSessions>,
    id: String,
    trust: Option<String>,
    remember: Option<bool>,
    width: u16,
    height: u16,
    scale: Option<u32>,
    on_event: Channel<InvokeResponseBody>,
) -> Result<String, String> {
    let c = find(&path, &id)?;
    let port = c.rdp_port.ok_or("Desktop über RDP ist für diese Verbindung ausgeschaltet")?;
    let (user, password) = rdp_login(&path, &c)?;
    let domain = (!c.rdp_domain.trim().is_empty()).then(|| c.rdp_domain.trim().to_string());
    let gateway = match c.rdp_gateway.trim() {
        "" => None,
        gw if c.gateway_credential_id.is_some() => {
            let (u, p) = cred_login(&path, c.gateway_credential_id.as_deref().unwrap_or_default())?;
            Some(rdp::Gateway { endpoint: gw.to_string(), user: u, password: p.unwrap_or_default() })
        }
        gw if c.rdp_gateway_user.trim().is_empty() => Some(rdp::Gateway {
            endpoint: gw.to_string(),
            // «Gleiche wie Verbindung»: Domäne wie bei mstsc vor den Benutzer.
            user: match &domain {
                Some(d) if !user.contains(['@', '\\']) => format!("{d}\\{}", user),
                _ => user.clone(),
            },
            password: password.clone(),
        }),
        gw => {
            let pw = vault::get(&c.id, Kind::GatewayPassword)?.unwrap_or_default();
            diag::add_secret(&pw);
            Some(rdp::Gateway { endpoint: gw.to_string(), user: c.rdp_gateway_user.trim().to_string(), password: pw })
        }
    };
    let pins_path = path.0.with_file_name("known_certs.json");
    let mut pins = load_pins(&pins_path);
    let pin_key = format!("{}:{port}", c.address);
    let (via_host, via_port, tunnel) = desktop_via_ssh(&c, port).await?;
    let mut tunnel = TunnelGuard(tunnel);
    let target = rdp::Target {
        host: via_host,
        port: via_port,
        user,
        domain,
        password,
        expected_cert: trust.clone().or_else(|| pins.get(&pin_key).cloned()),
        skip_cert_check: c.skip_cert_check,
        width: width.clamp(200, 8192),
        height: height.clamp(200, 8192),
        scale: scale.unwrap_or(100).clamp(100, 500),
        clipboard: None,
        audio: c.play_audio,
        share: (!c.shared_folder.trim().is_empty()).then(|| PathBuf::from(c.shared_folder.trim())),
        kdc: kdc::kdc_url(&c.rdp_kdc)?,
        no_ntlm: c.rdp_no_ntlm,
        gateway,
    };
    let sid = new_sid(&c.id);
    touch(&path, &c.id);
    let (tx, rx) = rdp::channel();
    let mut target = target;
    if c.shared_clipboard {
        target.clipboard = rdp_clipboard(&app, sid.clone(), tx.clone()).await;
    }
    let seen: rdp::SeenCert = Default::default();
    let on_event = relay::Relay::new(on_event);
    relay::DESKTOPS.lock().unwrap().insert(sid.clone(), on_event.clone());
    let opened = rdp::open(target, seen.clone(), (tx, rx), move |e| screen_frame(e).is_none_or(|f| on_event.send(f).is_ok())).await;
    let session = match opened {
        Ok(s) => s,
        Err(e) => {
            relay::DESKTOPS.lock().unwrap().remove(&sid);
            rdp_clipboard_drop(&app, sid);
            return Err(e);
        }
    };
    if let Some(t) = tunnel.0.take() {
        TUNNELS.lock().unwrap().insert(sid.clone(), t);
    }
    if let Some(fp) = trust.filter(|fp| remember.unwrap_or(true) && seen.lock().unwrap().as_ref() == Some(fp)) {
        pins.insert(pin_key, fp);
        let _ = std::fs::write(&pins_path, serde_json::to_string_pretty(&pins).unwrap_or_default());
    }
    sessions.0.lock().unwrap().insert(sid.clone(), session);
    Ok(sid)
}

fn rdp_send(s: &RdpSessions, sid: &str, i: rdp::Input) -> Result<(), String> {
    s.0.lock().unwrap().get(sid).map(|x| x.send(i)).ok_or_else(|| "RDP-Sitzung geschlossen".into())
}

#[tauri::command]
fn rdp_pointer(sessions: State<RdpSessions>, sid: String, x: u16, y: u16, buttons: u8) -> Result<(), String> {
    rdp_send(&sessions, &sid, rdp::Input::Pointer { x, y, buttons })
}

#[tauri::command]
fn rdp_wheel(sessions: State<RdpSessions>, sid: String, vertical: bool, units: i16) -> Result<(), String> {
    rdp_send(&sessions, &sid, rdp::Input::Wheel { vertical, units })
}

/// Scancode oder Unicode je nach Taste (siehe `rdp::key_input`).
#[tauri::command]
fn rdp_key(sessions: State<RdpSessions>, sid: String, code: String, key: String, down: bool, altgr: bool) -> Result<(), String> {
    match rdp::key_input(&code, &key, down, altgr) {
        Some(i) => rdp_send(&sessions, &sid, i),
        None => Ok(()),
    }
}

#[tauri::command]
fn rdp_set_monitors(sessions: State<RdpSessions>, sid: String, monitors: Vec<rdp::Monitor>) -> Result<(), String> {
    rdp_send(&sessions, &sid, rdp::Input::Monitors(monitors))
}

#[tauri::command]
fn rdp_release_all(sessions: State<RdpSessions>, sid: String) -> Result<(), String> {
    rdp_send(&sessions, &sid, rdp::Input::ReleaseAll)
}

#[tauri::command]
fn rdp_set_size(sessions: State<RdpSessions>, sid: String, width: u16, height: u16, scale: Option<u32>) -> Result<(), String> {
    rdp_send(&sessions, &sid, rdp::Input::Resize { width, height, scale: scale.unwrap_or(100).clamp(100, 500) })
}

#[tauri::command]
fn rdp_close(app: AppHandle, sessions: State<RdpSessions>, sid: String) {
    tunnel_end(&sid);
    relay::DESKTOPS.lock().unwrap().remove(&sid);
    if let Some(s) = sessions.0.lock().unwrap().remove(&sid) {
        s.close();
    }
    rdp_clipboard_drop(&app, sid);
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Hilfsprozess für die Windows-Tastenkombinationen (siehe kbhook), startet ohne Fenster und ohne Tauri.
    if std::env::args().nth(1).as_deref() == Some("--kbhook-helper") {
        return kbhook::helper(std::env::args().nth(2).and_then(|p| p.parse().ok()).unwrap_or(0));
    }
    let builder = tauri::Builder::default();
    #[cfg(target_os = "macos")]
    let builder = builder.register_uri_scheme_protocol("sfsymbol", |_ctx, req| {
        // sfsymbol://localhost/<name>?<pt>
        let name = req.uri().path().trim_start_matches('/');
        let pt = req.uri().query().and_then(|q| q.parse().ok()).filter(|p| (8.0..=64.0).contains(p)).unwrap_or(16.0);
        let ok = name.len() < 64 && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '.');
        match ok.then(|| sfsymbol::png(name, pt)).flatten() {
            Some(png) => tauri::http::Response::builder().header("Content-Type", "image/png").header("Cache-Control", "max-age=31536000")
                // CSS-Masken laden im CORS-Modus, ohne diese Freigabe bleibt das Symbol leer
                .header("Access-Control-Allow-Origin", "*").body(png),
            None => tauri::http::Response::builder().status(404).body(Vec::new()),
        }
        .unwrap()
    });
    // Zweiter Start (z. B. Klick auf eine Verknüpfung): Link an die laufende App, Fenster nach vorne. Nur Desktop.
    #[cfg(desktop)]
    let builder = builder.plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
        if let Some(w) = app.get_webview_window("main") {
            let _ = w.show();
            let _ = w.unminimize();
            let _ = w.set_focus();
        }
    }));
    builder
        .plugin(tauri_plugin_deep_link::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            // Menüs und eigene Titelleiste gibt es nur auf dem Desktop (iPadOS: Vollbild-App ohne Menüleiste).
            #[cfg(desktop)]
            {
            use tauri::menu::{MenuBuilder, MenuItemBuilder, SubmenuBuilder};
            // Menüs nach Systemstandard (docs/beschriftungen.md). macOS: native Menüleiste oben am Bildschirm.
            // Windows/Linux: eigene Titelleiste mit Menüknopf (src/titlebar.ts), kein natives Menü.
            if cfg!(target_os = "macos") {
                use tauri::menu::{AboutMetadata, PredefinedMenuItem as P};
                let about = AboutMetadata { name: Some("Sesam Remote".into()), version: Some(app.package_info().version.to_string()), ..Default::default() };
                let app_menu = SubmenuBuilder::new(app, "Sesam Remote")
                    .item(&P::about(app, Some("Über Sesam Remote"), Some(about))?)
                    .separator()
                    .item(&MenuItemBuilder::with_id("cmd:settings", "Einstellungen …").accelerator("CmdOrCtrl+,").build(app)?)
                    .text("cmd:credentials", "Zugangsdaten …")
                    .text("cmd:permissions", "Berechtigungen einrichten …")
                    .separator()
                    .item(&P::services(app, Some("Dienste"))?)
                    .separator()
                    .item(&P::hide(app, Some("Sesam Remote ausblenden"))?)
                    .item(&P::hide_others(app, Some("Andere ausblenden"))?)
                    .item(&P::show_all(app, Some("Alle einblenden"))?)
                    .separator()
                    .item(&P::quit(app, Some("Sesam Remote beenden"))?)
                    .build()?;
                let file = SubmenuBuilder::new(app, "Ablage")
                    .item(&MenuItemBuilder::with_id("cmd:new", "Neue Verbindung …").accelerator("CmdOrCtrl+N").build(app)?)
                    .item(&MenuItemBuilder::with_id("cmd:quick", "Schnelle Verbindung …").accelerator("CmdOrCtrl+K").build(app)?)
                    .item(&MenuItemBuilder::with_id("cmd:terminal", "Neues Terminal").accelerator("CmdOrCtrl+T").build(app)?)
                    .text("cmd:addubuntu", "Ubuntu-PC hinzufügen …")
                    .separator()
                    // ⌘W wie in Safari: aktiven Tab schliessen, in der Bibliothek das Fenster (src/main.ts «close»)
                    .item(&MenuItemBuilder::with_id("cmd:close", "Schliessen").accelerator("CmdOrCtrl+W").build(app)?)
                    .build()?;
                let edit = SubmenuBuilder::new(app, "Bearbeiten")
                    .item(&P::undo(app, Some("Widerrufen"))?)
                    .item(&P::redo(app, Some("Wiederholen"))?)
                    .separator()
                    .item(&P::cut(app, Some("Ausschneiden"))?)
                    .item(&P::copy(app, Some("Kopieren"))?)
                    .item(&P::paste(app, Some("Einsetzen"))?)
                    .item(&P::select_all(app, Some("Alles auswählen"))?)
                    .build()?;
                // Befehlspalette ohne Kürzel im Menü: ⌘⇧P bleibt beim Remote-Bild, wenn es die Tastatur hat (src/palette.ts).
                let view = SubmenuBuilder::new(app, "Darstellung")
                    .text("cmd:palette", "Befehlspalette …")
                    .separator()
                    .text("tool:Bibliothek", "Bibliothek anzeigen")
                    .text("tool:Vorheriger Tab", "Vorheriger Tab")
                    .text("tool:Nächster Tab", "Nächster Tab")
                    .separator()
                    .item(&P::fullscreen(app, Some("Vollbild"))?)
                    .build()?;
                // Gleiche Beschriftungen wie die Werkzeugknöpfe (src/tabs.ts runTool); Kürzel bleiben bei der Sitzung.
                let session = SubmenuBuilder::new(app, "Sitzung")
                    .text("tool:Neue Shell", "Neue Shell")
                    .text("tool:Suchen", "Suchen …")
                    .text("tool:Zwischenablage abtippen", "Zwischenablage abtippen")
                    .text("tool:Bildschirmfoto", "Bildschirmfoto")
                    .text("tool:Alle Monitore", "Alle Monitore")
                    .separator()
                    .text("tool:Abkoppeln", "In eigenem Fenster öffnen")
                    .text("tool:Andocken", "Ins Hauptfenster zurück")
                    .text("tool:Minimieren", "Minimieren")
                    .text("tool:Trennen", "Trennen")
                    .build()?;
                let window = SubmenuBuilder::new(app, "Fenster")
                    .item(&P::minimize(app, Some("Im Dock ablegen"))?)
                    .item(&P::maximize(app, Some("Zoomen"))?)
                    .build()?;
                // IDs wie in src/help.ts.
                let help = SubmenuBuilder::new(app, "Hilfe")
                    .text("help:zugangsdaten", "Welche Zugangsdaten?")
                    .separator()
                    .text("help:rdp-gnome", "RDP mit GNOME (Ubuntu)")
                    .text("help:rdp-windows", "RDP mit Windows")
                    .text("help:rdp-xrdp", "RDP mit xrdp (Linux)")
                    .text("help:komfort", "Komfortfunktionen")
                    .text("help:vnc", "VNC allgemein")
                    .text("help:wayvnc", "wayvnc (wlroots)")
                    .text("help:macos", "macOS-Bildschirmfreigabe")
                    .text("help:ssh", "SSH und SFTP")
                    .text("help:ftp", "FTP und FTPS")
                    .build()?;
                app.set_menu(MenuBuilder::new(app).items(&[&app_menu, &file, &edit, &view, &session, &window, &help]).build()?)?;
            } else if let Some(w) = app.get_webview_window("main") {
                w.set_decorations(false)?;
                let _ = w.set_shadow(true);
            }
            }
            // Nur Debug-Build: Testskript aus $SESAM_DIAG_JS (Datei) im Hauptfenster ausführen, dann löschen; für Tests ohne Maus/Tastatur.
            #[cfg(debug_assertions)]
            if let Ok(path) = std::env::var("SESAM_DIAG_JS") {
                let h = app.handle().clone();
                std::thread::spawn(move || loop {
                    std::thread::sleep(std::time::Duration::from_millis(500));
                    // Erste Zeile «@fenster» wählt ein anderes Fenster (z. B. @settings), sonst das Hauptfenster.
                    if let Ok(js) = std::fs::read_to_string(&path) {
                        let (label, code) = match js.strip_prefix('@').and_then(|r| r.split_once('\n')) {
                            Some((l, c)) => (l.trim().to_string(), c.to_string()),
                            None => ("main".to_string(), js),
                        };
                        if let Some(w) = h.get_webview_window(&label) {
                            let _ = std::fs::remove_file(&path);
                            let _ = w.eval(&code);
                        }
                    }
                });
            }
            #[cfg(desktop)]
            app.on_menu_event(|app, e| {
                if let Some(topic) = e.id().as_ref().strip_prefix("help:") {
                    let _ = app.emit("help", topic);
                } else if let Some(cmd) = e.id().as_ref().strip_prefix("cmd:") {
                    let _ = app.emit("cmd", cmd);
                } else if let Some(tool) = e.id().as_ref().strip_prefix("tool:") {
                    let _ = app.emit("tool", tool);
                }
            });
            #[cfg(any(windows, target_os = "linux"))]
            {
                use tauri_plugin_deep_link::DeepLinkExt;
                let _ = app.deep_link().register_all();
            }
            let dir = app.path().app_data_dir()?;
            diag::init(&dir);
            kbhook::init(app.handle().clone());
            let _ = LIBRARY_FILE.set(dir.join("library.json"));
            app.manage(LibraryPath(dir.join("library.json")));
            app.manage(ssh::Shells::default());
            app.manage(Desktops::default());
            app.manage(Files::default());
            app.manage(RdpSessions::default());
            app.manage(Transfers::default());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            library_list,
            library_save,
            library_delete,
            groups_list,
            groups_save,
            group_rename,
            group_delete,
            shell_open,
            shell_write,
            shell_resize,
            shell_probe,
            shell_held_list,
            shell_upload,
            shell_door_open,
            conn_diagnose,
            desktop_detect,
            desktop_enable,
            shell_door_list,
            shell_door_close,
            shell_held_kill,
            conn_set_persist,
            conn_set_os,
            shell_close,
            vnc_open,
            desktop_ack,
            open_settings_window,
            save_text_file,
            perm_list,
            library_merge,
            probe_services,
            perm_request,
            perm_open,
            vnc_pointer,
            vnc_key,
            vnc_set_size,
            vnc_clipboard_sync,
            vnc_close,
            sftp_open,
            sftp_list,
            sftp_mkdir,
            sftp_rename,
            sftp_remove,
            sftp_close,
            app_exit,
            clip_read,
            clip_write,
            kbhook_set,
            kbhook_selftest,
            conn_reach,
            conn_wake,
            term_profiles,
            term_open,
            rdp_check,
            rdp_ports,
            desktop_attach,
            shell_attach,
            open_detached,
            sftp_chmod,
            sftp_touch,
            sftp_owner,
            sftp_set_perms,
            sftp_tempdir,
            sftp_sudo_move,
            sftp_sudo_op,
            sftp_trash,
            sftp_trash_slot,
            sftp_restore,
            sftp_trash_cleanup,
            sftp_size,
            sftp_sha256,
            local_sha256,
            local_mkdir,
            local_rename,
            local_list,
            save_bytes,
            certs_list,
            quick_register,
            creds_list,
            cred_test,
            backup_export,
            backup_preview,
            backup_import,
            cred_generate_key,
            cred_public_key,
            cred_install_key,
            cred_reveal,
            cred_duplicate,
            cred_save,
            cred_delete,
            cred_from_connection,
            create_shortcut,
            drop_files,
            thumb_save,
            thumb_get,
            thumb_import,
            setup_script,
            setup_upload,
            cert_remove,
            local_home,
            transfer_start,
            transfer_cancel,
            rdp_open,
            rdp_pointer,
            rdp_wheel,
            rdp_key,
            rdp_release_all,
            rdp_set_monitors,
            rdp_set_size,
            rdp_close
        ])
        .build(tauri::generate_context!())
        .expect("Sesam Remote konnte nicht starten")
        .run(|_app, _e| {
            // macOS: Klick aufs Dock-Symbol bei ausgeblendetem Hauptfenster (rotes Schliessen) holt es zurück.
            #[cfg(target_os = "macos")]
            if let tauri::RunEvent::Reopen { has_visible_windows: false, .. } = _e {
                if let Some(w) = _app.get_webview_window("main") {
                    let _ = w.show();
                    let _ = w.set_focus();
                }
            }
        });
}

#[cfg(test)]
mod setup_script_tests {
    /// Das Setup-Script wird unverändert ausgeliefert (LF, ohne BOM); die Summe muss zur freigegebenen Fassung passen.
    #[test]
    fn setup_script_is_the_released_version() {
        let (bytes, sha) = super::setup_script();
        assert_eq!(sha, "066c8bbdf4ca6173713f3e21f46d1fa695aac8d1815e388dc2fe868492571b7a");
        assert_eq!(bytes.len(), 17656);
        assert!(!bytes.starts_with(&[0xEF, 0xBB, 0xBF]) && !bytes.windows(2).any(|w| w == b"\r\n"));
    }
}
