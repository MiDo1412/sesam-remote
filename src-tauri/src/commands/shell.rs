//! SSH-Shell und lokale Terminals.

use crate::*;

/// Pfad der Bibliothek für Stellen ohne Tauri-State (Sprungserver nachschlagen).
pub(crate) static LIBRARY_FILE: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();

pub(crate) fn ssh_target(c: &Connection, trust: Option<String>) -> Result<ssh::Target, String> {
    let mut t = ssh_target_direct(c, trust)?;
    if !c.jump_host.is_empty() {
        let path = LIBRARY_FILE.get().ok_or("Bibliothek nicht geladen")?;
        let j = find(&LibraryPath(path.clone()), &c.jump_host).map_err(|_| "Sprungserver nicht mehr in der Bibliothek".to_string())?;
        // Nur eine Stufe: der Sprungserver selbst verbindet direkt.
        t.jump = Some(Box::new(ssh_target_direct(&j, None)?));
    }
    Ok(t)
}

pub(crate) fn ssh_target_direct(c: &Connection, trust: Option<String>) -> Result<ssh::Target, String> {
    let (password, key) = (login_secret(c, Kind::Password)?, login_secret(c, Kind::SshKey)?);
    // Laut Bibliothek gespeichert, im Schlüsselbund aber nicht (anderer Build, Gerät gewechselt, Sicherung ohne Geheimnisse):
    // klar sagen statt nur «Anmeldung abgelehnt».
    let missing = [(c.has_password && c.credential_id.is_none() && password.is_none(), "Passwort"), (c.has_key && c.credential_id.is_none() && key.is_none(), "SSH-Schlüssel")];
    for (gone, what) in missing {
        if gone {
            tracing::warn!(conn = %c.id, "{what} laut Bibliothek gespeichert, im Schlüsselbund aber nicht gefunden");
        }
    }
    if password.is_none() && key.is_none() && missing.iter().any(|(g, _)| *g) && !c.ssh_agent {
        let what = match (missing[0].0, missing[1].0) {
            (true, true) => "Passwort und der SSH-Schlüssel fehlen",
            (true, false) => "Passwort fehlt",
            _ => "SSH-Schlüssel fehlt",
        };
        let art = if missing[0].0 { "Das gespeicherte" } else { "Der gespeicherte" };
        return Err(format!("{art} {what} im Schlüsselbund dieses Geräts (Verbindung «{}»). Bitte unter «Bearbeiten» neu eintragen.", c.name));
    }
    Ok(ssh::Target {
        host: c.address.clone(),
        port: c.ssh_port.ok_or_else(|| format!("SSH ist für «{}» ausgeschaltet", c.name))?,
        user: c.user.clone(),
        password,
        key,
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
pub(crate) async fn shell_open(
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
pub(crate) async fn shell_write(shells: State<'_, ssh::Shells>, sid: String, data: Vec<u8>) -> Result<(), String> {
    if let Some(t) = local_term(&sid) {
        return t.write(&data);
    }
    shells.0.lock().await.get(&sid).ok_or("Shell geschlossen")?.write(data).await
}

/// Lokales Terminal zu dieser Sitzungs-ID (die Shell-Befehle bedienen SSH und lokal gleich).
pub(crate) fn local_term(sid: &str) -> Option<Arc<localterm::Term>> {
    localterm::TERMS.lock().unwrap().get(sid).cloned()
}

/// Lokales Terminal mit einem Profil starten (PowerShell, cmd, WSL, zsh, Python …).
#[tauri::command]
pub(crate) fn term_open(profile: localterm::Spec, cols: u16, rows: u16, on_event: Channel<ssh::ShellEvent>) -> Result<String, String> {
    let relay = relay::Relay::new(on_event);
    let term = localterm::open(&profile, cols, rows, relay.clone())?;
    let sid = new_sid("local");
    localterm::TERMS.lock().unwrap().insert(sid.clone(), term);
    relay::SHELLS.lock().unwrap().insert(sid.clone(), relay);
    Ok(sid)
}

/// Erkanntes Betriebssystem eintragen (nur wenn noch keins gewählt ist), für das Symbol in der Bibliothek.
#[tauri::command]
pub(crate) fn conn_set_os(path: State<LibraryPath>, id: String, os: String) -> Result<bool, String> {
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
pub(crate) async fn shell_upload(shells: State<'_, ssh::Shells>, sid: String, files: Vec<String>, dir: String) -> Result<Vec<String>, String> {
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
pub(crate) static DOORS: std::sync::LazyLock<Mutex<HashMap<String, Vec<(u16, bool, u16, tokio::task::JoinHandle<()>)>>>> = std::sync::LazyLock::new(Default::default);

/// `port` auf dem Server (localhost) über die SSH-Verbindung dieser Shell erreichbar machen; gibt den lokalen Port
/// auf 127.0.0.1 zurück. Ist die Tür schon offen, dieselbe.
#[tauri::command]
pub(crate) async fn shell_door_open(shells: State<'_, ssh::Shells>, sid: String, port: u16, v6: bool) -> Result<u16, String> {
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
pub(crate) fn shell_door_list(sid: String) -> Vec<(u16, bool, u16)> {
    DOORS.lock().unwrap().get(&sid).map(|v| v.iter().map(|d| (d.0, d.1, d.2)).collect()).unwrap_or_default()
}

/// Tür schliessen (`port` 0: alle dieser Shell).
#[tauri::command]
pub(crate) fn shell_door_close(sid: String, port: u16) {
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
pub(crate) async fn shell_held_list(shells: State<'_, ssh::Shells>, sid: String) -> Result<Vec<(String, bool, u64)>, String> {
    let h = shells.0.lock().await.get(&sid).ok_or("Shell geschlossen")?.handle();
    ssh::tmux_list(&h).await
}

#[tauri::command]
pub(crate) async fn shell_held_kill(shells: State<'_, ssh::Shells>, sid: String, name: String) -> Result<(), String> {
    let h = shells.0.lock().await.get(&sid).ok_or("Shell geschlossen")?.handle();
    ssh::tmux_kill(&h, &name).await
}

/// Shell dieser Verbindung künftig halten (tmux) bzw. nicht mehr.
#[tauri::command]
pub(crate) fn conn_set_persist(path: State<LibraryPath>, id: String, on: bool) -> Result<(), String> {
    let _g = LIBRARY_LOCK.lock().unwrap();
    let mut lib = library::load(&path.0)?;
    let c = lib.iter_mut().find(|c| c.id == id).ok_or("Verbindung nicht gefunden")?;
    c.persist_shell = if on { "tmux".into() } else { String::new() };
    library::save(&path.0, &lib)
}

/// Rechner der Shell befragen (Raspberry-Pi-Erkennung, Temperatur, Unterspannung).
#[tauri::command]
pub(crate) async fn shell_probe(shells: State<'_, ssh::Shells>, sid: String) -> Result<ssh::Probe, String> {
    let h = shells.0.lock().await.get(&sid).ok_or("Shell geschlossen")?.handle();
    ssh::probe(&h).await
}

#[tauri::command]
pub(crate) async fn shell_resize(shells: State<'_, ssh::Shells>, sid: String, cols: u32, rows: u32) -> Result<(), String> {
    if let Some(t) = local_term(&sid) {
        return t.resize(cols.min(u16::MAX as u32) as u16, rows.min(u16::MAX as u32) as u16);
    }
    shells.0.lock().await.get(&sid).ok_or("Shell geschlossen")?.resize(cols, rows).await
}

#[tauri::command]
pub(crate) async fn shell_close(shells: State<'_, ssh::Shells>, sid: String) -> Result<(), String> {
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

/// Antwort auf eine Rückfrage des SSH-Servers (Einmalcode usw.), `None` = abgebrochen.
#[tauri::command]
pub(crate) fn ssh_prompt_answer(id: u64, answers: Option<Vec<String>>) {
    ssh::answer(id, answers);
}
