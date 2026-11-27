//! Verbindungen nachschlagen, prüfen und Zugangsdaten.

use crate::*;

pub(crate) fn find(path: &LibraryPath, id: &str) -> Result<Connection, String> {
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
pub(crate) fn rdp_login(path: &LibraryPath, c: &Connection) -> Result<(String, String), String> {
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

/// RD-Gateway der Verbindung samt Anmeldung (eigene Zugangsdaten, eigenes Passwort oder die der Verbindung).
pub(crate) fn rdp_gateway(path: &LibraryPath, c: &Connection, user: &str, domain: &Option<String>, password: &str) -> Result<Option<rdp::Gateway>, String> {
    Ok(match c.rdp_gateway.trim() {
        "" => None,
        gw if c.gateway_credential_id.is_some() => {
            let (u, p) = cred_login(path, c.gateway_credential_id.as_deref().unwrap_or_default())?;
            Some(rdp::Gateway { endpoint: gw.to_string(), user: u, password: p.unwrap_or_default() })
        }
        gw if c.rdp_gateway_user.trim().is_empty() => Some(rdp::Gateway {
            endpoint: gw.to_string(),
            // «Gleiche wie Verbindung»: Domäne wie bei mstsc vor den Benutzer.
            user: match domain {
                Some(d) if !user.contains(['@', '\\']) => format!("{d}\\{user}"),
                _ => user.to_string(),
            },
            password: password.to_string(),
        }),
        gw => {
            let pw = vault::get(&c.id, Kind::GatewayPassword)?.unwrap_or_default();
            diag::add_secret(&pw);
            Some(rdp::Gateway { endpoint: gw.to_string(), user: c.rdp_gateway_user.trim().to_string(), password: pw })
        }
    })
}

/// Host und Port eines RD-Gateways (`host`, `host:8443`, `[::1]:443`); Standard 443.
pub(crate) fn gateway_addr(endpoint: &str) -> (String, u16) {
    let e = endpoint.trim();
    if let Some(rest) = e.strip_prefix('[')
        && let Some((h, tail)) = rest.split_once(']')
    {
        return (h.to_string(), tail.strip_prefix(':').and_then(|p| p.parse().ok()).unwrap_or(443));
    }
    match e.rsplit_once(':') {
        Some((h, p)) if !h.contains(':') => (h.to_string(), p.parse().unwrap_or(443)),
        _ => (e.to_string(), 443),
    }
}

/// «Zugangsdaten prüfen»: nur bis zur Anmeldung (NLA), dann sauber trennen. Zertifikat muss schon bestätigt sein.
#[tauri::command]
pub(crate) async fn rdp_check(path: State<'_, LibraryPath>, id: String, trust: Option<String>, remember: Option<bool>) -> Result<(), String> {
    let c = find(&path, &id)?;
    let port = c.rdp_port.ok_or("RDP ist für diese Verbindung ausgeschaltet")?;
    let (user, password) = rdp_login(&path, &c)?;
    let pins_path = path.0.with_file_name("known_certs.json");
    let pins = load_pins(&pins_path);
    let pin_key = format!("{}:{port}", c.address);
    let domain = (!c.rdp_domain.trim().is_empty()).then(|| c.rdp_domain.trim().to_string());
    let gateway = rdp_gateway(&path, &c, &user, &domain, &password)?;
    let t = rdp::Target {
        host: c.address.clone(),
        server_name: c.address.clone(),
        port,
        user,
        domain,
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
        gateway,
        mic: None,
    };
    let seen: rdp::SeenCert = Default::default();
    rdp::check_login(t, seen.clone()).await?;
    // Wie beim Verbinden: bestätigtes Zertifikat merken, damit die nächste Verbindung nicht erneut fragt.
    if let Some(fp) = trust.filter(|fp| remember.unwrap_or(true) && seen.lock().unwrap().as_ref() == Some(fp)) {
        let _ = save_pins(&pins_path, &pin_key, Some(fp));
    }
    Ok(())
}

/// Erste Hilfe: Verbindung für einen Dienst (ssh, rdp, vnc, ftp) Schritt für Schritt prüfen; jeder Schritt geht
/// sofort über `on_step` an die Oberfläche. Anmeldung und Zertifikat mit den gespeicherten Zugangsdaten,
/// ohne etwas zu bestätigen oder zu speichern.
#[tauri::command]
pub(crate) async fn conn_diagnose(path: State<'_, LibraryPath>, id: String, service: String, on_step: Channel<firstaid::Step>) -> Result<(), String> {
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
    } else if service == "rdp" && !c.rdp_gateway.trim().is_empty() {
        // Über ein RD-Gateway ist das interne Ziel von hier aus meist gar nicht erreichbar: das Gateway prüfen.
        let (gh, gp) = gateway_addr(&c.rdp_gateway);
        send(step("name", "RD-Gateway", None, format!("Der Desktop läuft über das RD-Gateway {gh}:{gp}; geprüft wird zuerst das Gateway."), ""));
        if !firstaid::network(&gh, gp, "gateway", &send).await {
            return Ok(());
        }
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
        "rdp" | "vnc" | "ftp" => match match service.as_str() {
            "rdp" => rdp_check(path, id, None, Some(false)).await,
            "vnc" => vnc_login(&path, &c, None, Some(false)).await.map(drop),
            _ => ftp_open(&path, &c, None, false).await.map(drop),
        } {
            Ok(()) => {
                if service == "rdp" {
                    send(step("cert", "Zertifikat", Some(true), "Bekannt oder gültig.", ""));
                }
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
pub(crate) async fn desktop_detect(path: State<'_, LibraryPath>, id: String, trust: Option<String>) -> Result<unlock::Plan, String> {
    let c = find(&path, &id)?;
    Ok(unlock::plan(&ssh::exec(&ssh_target(&c, trust)?, unlock::DETECT).await?))
}

/// Schritt 2, nach Bestätigung: die festen Befehle des Systems mit sudo ausführen (Ubuntu: Setup-Script läuft in der
/// Shell, hier nur eintragen) und den Desktop-Dienst in der Verbindung eintragen. `SUDO_PASSWORD`: Passwort nötig.
#[tauri::command]
pub(crate) async fn desktop_enable(path: State<'_, LibraryPath>, id: String, trust: Option<String>, system: String, password: Option<String>) -> Result<(), String> {
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
pub(crate) async fn rdp_ports(
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
pub(crate) fn cred_login(path: &LibraryPath, cid: &str) -> Result<(String, Option<String>), String> {
    let cred = creds::load(&creds_path(path))?.into_iter().find(|x| x.id == cid).ok_or("Zugewiesene Zugangsdaten gibt es nicht mehr")?;
    cred_used(cid);
    let pw = vault::get(&creds::vault_id(cid), Kind::Password)?;
    if let Some(p) = &pw {
        diag::add_secret(p);
    }
    Ok((cred.user, pw))
}

pub(crate) fn creds_path(path: &LibraryPath) -> PathBuf {
    path.0.with_file_name("credentials.json")
}

/// Zugangsdaten mit den Namen der Verbindungen, die sie nutzen.
#[tauri::command]
pub(crate) fn creds_list(path: State<LibraryPath>) -> Result<Vec<(creds::Cred, Vec<String>)>, String> {
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
pub(crate) fn cred_generate_key(path: State<LibraryPath>, id: String) -> Result<String, String> {
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
pub(crate) fn cred_public_key(id: String) -> Result<String, String> {
    let k = vault::get(&creds::vault_id(&id), Kind::SshKey)?.ok_or("Diese Zugangsdaten haben keinen SSH-Schlüssel")?;
    ssh::public_key_of(&k)
}

/// Öffentlichen Schlüssel auf einem Server eintragen (wie ssh-copy-id), angemeldet mit den bisherigen Zugangsdaten der Verbindung.
#[tauri::command]
pub(crate) async fn cred_install_key(path: State<'_, LibraryPath>, id: String, connection: String, trust: Option<String>) -> Result<String, String> {
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

/// Gespeichertes Passwort anzeigen bzw. kopieren: nur nach Touch ID/Face ID bzw. Passwort des Geräts.
#[tauri::command]
pub(crate) async fn cred_reveal(window: tauri::WebviewWindow, path: State<'_, LibraryPath>, id: String) -> Result<String, String> {
    let cred = creds::load(&creds_path(&path))?.into_iter().find(|x| x.id == id).ok_or("Zugangsdaten nicht gefunden")?;
    let _ = &window;
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    {
        let reason = format!("das Passwort von «{}» anzeigen", cred.name);
        tauri::async_runtime::spawn_blocking(move || auth::authenticate(&reason)).await.map_err(|e| e.to_string())??;
    }
    #[cfg(windows)]
    {
        let hwnd = window.hwnd().map_err(|e| e.to_string())?.0 as isize;
        let reason = format!("Passwort von «{}» anzeigen", cred.name);
        if !tauri::async_runtime::spawn_blocking(move || auth::hello(hwnd, &reason)).await.map_err(|e| e.to_string())?? {
            return Err("Zum Anzeigen braucht es Windows Hello (Einstellungen › Konten › Anmeldeoptionen).".into());
        }
    }
    // Linux: kein einheitlicher Systemdialog zum Bestätigen, darum dort gesperrt.
    #[cfg(not(any(target_os = "macos", target_os = "ios", windows)))]
    {
        let _ = &cred;
        return Err("Anzeigen ist auf diesem System noch nicht möglich".into());
    }
    #[allow(unreachable_code)]
    vault::get(&creds::vault_id(&id), Kind::Password)?.ok_or_else(|| "Kein Passwort gespeichert".into())
}

/// Kopie mit eigenem Namen, samt Geheimnissen.
#[tauri::command]
pub(crate) fn cred_duplicate(path: State<LibraryPath>, id: String, new_id: String, name: String) -> Result<creds::Cred, String> {
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
pub(crate) fn cred_save(path: State<LibraryPath>, mut cred: creds::Cred, password: Option<String>, ssh_key: Option<String>) -> Result<creds::Cred, String> {
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
    cred.updated_at = Some(now_secs());
    match all.iter_mut().find(|x| x.id == cred.id) {
        Some(x) => *x = cred.clone(),
        None => all.push(cred.clone()),
    }
    creds::save(&p, &all)?;
    Ok(cred)
}

/// Löschen; Verbindungen, die es nutzen, fallen auf «Eigene» zurück.
#[tauri::command]
pub(crate) fn cred_delete(path: State<LibraryPath>, id: String) -> Result<(), String> {
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
pub(crate) fn cred_from_connection(path: State<LibraryPath>, id: String, cred_id: String, name: String) -> Result<creds::Cred, String> {
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

pub(crate) fn find_raw(path: &LibraryPath, id: &str) -> Result<Connection, String> {
    library::load(&path.0)?.into_iter().find(|c| c.id == id).ok_or_else(|| "Verbindung nicht gefunden".into())
}

/// Passwort/SSH-Schlüssel: aus den gespeicherten Zugangsdaten, wenn zugewiesen, sonst aus der Verbindung.
pub(crate) fn login_secret(c: &Connection, kind: Kind) -> Result<Option<String>, String> {
    // Schnelle Verbindung: hier eingetippt/gewählt geht vor gespeicherten Zugangsdaten.
    if c.id.starts_with("quick-")
        && let Some(s) = vault::get(&c.id, kind)? {
            return Ok(Some(s));
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
pub(crate) fn cred_used(cid: &str) {
    let (Some(lib), Ok(_g)) = (LIBRARY_FILE.get(), LIBRARY_LOCK.try_lock()) else { return };
    let p = creds_path(&LibraryPath(lib.clone()));
    let Ok(mut all) = creds::load(&p) else { return };
    if let Some(c) = all.iter_mut().find(|x| x.id == cid) {
        c.last_used = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).ok().map(|d| d.as_secs());
        let _ = creds::save(&p, &all);
    }
}

/// Zugang testen: mit diesen Zugangsdaten an einer Verbindung anmelden, die sie nutzt, und gleich wieder trennen.
/// Der Dienst folgt der Zuweisung: RDP- bzw. VNC-Zugangsdaten über RDP bzw. VNC, sonst SSH, ohne SSH über FTP.
/// `trust` ist je nach Dienst der Hostschlüssel (SSH) oder das Zertifikat (RDP, VNC, FTPS); es wird gemerkt.
#[tauri::command]
pub(crate) async fn cred_test(path: State<'_, LibraryPath>, connection: String, credential: Option<String>, trust: Option<String>) -> Result<String, String> {
    let c = find(&path, &connection)?;
    let is = |x: &Option<String>| credential.is_some() && x == &credential;
    let via = if is(&c.rdp_credential_id) {
        rdp_check(path.clone(), connection, trust, Some(true)).await?;
        "RDP"
    } else if is(&c.vnc_credential_id) {
        let (_stream, _init, _tunnel) = vnc_login(&path, &c, trust, Some(true)).await?;
        "VNC"
    } else if c.ssh_port.is_some() {
        let h = ssh::connect(&ssh_target(&c, trust)?).await?;
        let _ = h.disconnect(russh::Disconnect::ByApplication, "", "de").await;
        "SSH"
    } else if c.ftp_port.is_some() {
        drop(ftp_open(&path, &c, trust, true).await?);
        "FTP"
    } else {
        return Err(format!("«{}» hat weder SSH noch FTP; dort lässt sich der Zugang nicht testen.", c.name));
    };
    Ok(format!("Anmeldung an «{}» über {via} erfolgreich.", c.name))
}
