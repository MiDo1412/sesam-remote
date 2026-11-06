mod library;
mod ssh;
mod vault;
mod vnc;

use library::Connection;
use std::path::PathBuf;
use tauri::ipc::Channel;
use tauri::{Manager, State};
use vault::Kind;

struct LibraryPath(PathBuf);

/// Serialisiert Lese-Ändern-Schreiben der Bibliothek.
static LIBRARY_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[tauri::command]
fn library_list(path: State<LibraryPath>) -> Result<Vec<Connection>, String> {
    library::load(&path.0)
}

/// `password`/`ssh_key`: `None` = unverändert, `Some("")` = löschen, sonst neu setzen.
#[tauri::command]
fn library_save(
    path: State<LibraryPath>,
    mut connection: Connection,
    password: Option<String>,
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
    connection.has_key = old.is_some_and(|c| c.has_key);
    for (secret, kind, flag) in [
        (password, Kind::Password, &mut connection.has_password),
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

#[tauri::command]
fn library_delete(path: State<LibraryPath>, id: String) -> Result<(), String> {
    let _g = LIBRARY_LOCK.lock().unwrap();
    let mut items = library::load(&path.0)?;
    if library::remove(&mut items, &id) {
        vault::delete(&id, Kind::Password)?;
        vault::delete(&id, Kind::SshKey)?;
        library::save(&path.0, &items)?;
    }
    Ok(())
}

#[tauri::command]
async fn shell_open(
    path: State<'_, LibraryPath>,
    shells: State<'_, ssh::Shells>,
    id: String,
    cols: u32,
    rows: u32,
    trust: Option<String>,
    on_event: Channel<ssh::ShellEvent>,
) -> Result<String, String> {
    let c = library::load(&path.0)?
        .into_iter()
        .find(|c| c.id == id)
        .ok_or("Verbindung nicht gefunden")?;
    let port = c.ssh_port.ok_or("SSH ist für diese Verbindung ausgeschaltet")?;
    let target = ssh::Target {
        host: &c.address,
        port,
        user: &c.user,
        password: vault::get(&c.id, Kind::Password)?,
        key: vault::get(&c.id, Kind::SshKey)?,
        trusted: trust,
    };
    let shell = ssh::open(target, cols, rows, on_event).await?;
    let sid = format!("{}-{}", c.id, std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos());
    shells.0.lock().await.insert(sid.clone(), shell);
    Ok(sid)
}

#[tauri::command]
async fn shell_write(shells: State<'_, ssh::Shells>, sid: String, data: Vec<u8>) -> Result<(), String> {
    shells.0.lock().await.get(&sid).ok_or("Shell geschlossen")?.write(data).await
}

#[tauri::command]
async fn shell_resize(shells: State<'_, ssh::Shells>, sid: String, cols: u32, rows: u32) -> Result<(), String> {
    shells.0.lock().await.get(&sid).ok_or("Shell geschlossen")?.resize(cols, rows).await
}

#[tauri::command]
async fn shell_close(shells: State<'_, ssh::Shells>, sid: String) -> Result<(), String> {
    if let Some(s) = shells.0.lock().await.remove(&sid) {
        s.close().await;
    }
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let dir = app.path().app_data_dir()?;
            app.manage(LibraryPath(dir.join("library.json")));
            app.manage(ssh::Shells::default());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            library_list,
            library_save,
            library_delete,
            shell_open,
            shell_write,
            shell_resize,
            shell_close
        ])
        .run(tauri::generate_context!())
        .expect("Sesam Remote konnte nicht starten");
}
