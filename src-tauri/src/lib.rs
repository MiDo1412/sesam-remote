#[cfg(any(target_os = "macos", target_os = "ios"))]
mod auth;
mod backup;
mod clipshare;
mod creds;
mod diag;
mod folders;
mod h264;
mod jumps;
mod kbhook;
mod kdc;
mod firstaid;
mod forward;
mod unlock;
mod ftp;
mod library;
mod perms;
mod portable;
#[cfg(desktop)]
mod update;
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

mod commands;
use commands::{library::*, conn::*, shell::*, files::*, system::*, updates::*, vnc::*, rdp::*};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Hilfsprozess für die Windows-Tastenkombinationen (siehe kbhook), startet ohne Fenster und ohne Tauri.
    if std::env::args().nth(1).as_deref() == Some("--kbhook-helper") {
        return kbhook::helper(std::env::args().nth(2).and_then(|p| p.parse().ok()).unwrap_or(0));
    }
    portable::prepare_webview();
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
    // Automatische Updates (src/update.rs), nur Desktop; iPad: App Store.
    #[cfg(desktop)]
    let builder = builder.plugin(tauri_plugin_updater::Builder::new().build());
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
                use tauri::menu::PredefinedMenuItem as P;
                let app_menu = SubmenuBuilder::new(app, "Sesam Remote")
                    // Eigener Bereich «Über Sesam» statt des Systemfensters: zeigt die Versionsangabe aus VERSION
                    // samt Entwicklungsstand (vite.config.ts), die das Systemfenster nicht kennt.
                    .text("cmd:about", "Über Sesam Remote")
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
                // IDs und Gruppen wie in src/help.ts. «help:» ohne Thema = Anleitung zur offenen Sitzung.
                let setup = SubmenuBuilder::new(app, "Zielrechner einrichten")
                    .text("help:rdp-windows", "Windows (RDP)")
                    .text("help:rdp-gnome", "Ubuntu mit GNOME (RDP)")
                    .text("help:rdp-xrdp", "Linux mit xrdp (RDP)")
                    .text("help:macos", "Mac mit Bildschirmfreigabe")
                    .text("help:wayvnc", "Linux mit wayvnc (wlroots)")
                    .build()?;
                let connect = SubmenuBuilder::new(app, "Verbinden")
                    .text("help:zugangsdaten", "Welche Zugangsdaten?")
                    .text("help:vnc", "VNC allgemein")
                    .text("help:ssh", "SSH und SFTP")
                    .text("help:ftp", "FTP und FTPS")
                    .build()?;
                let help = SubmenuBuilder::new(app, "Hilfe")
                    .text("help:", "Hilfe zu dieser Ansicht")
                    .separator()
                    .item(&setup)
                    .item(&connect)
                    .text("help:komfort", "Komfortfunktionen")
                    .separator()
                    .text("cmd:news", "Was ist neu in dieser Version")
                    .text("cmd:issue", "Fehler melden oder Idee teilen …")
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
            let dir = match portable::data_dir() {
                Some(d) => d,
                None => app.path().app_data_dir()?,
            };
            diag::init(&dir);
            kbhook::init(app.handle().clone());
            let _ = LIBRARY_FILE.set(dir.join("library.json"));
            if let Ok(c) = app.path().app_cache_dir() {
                let _ = std::fs::remove_dir_all(c.join("Ablage"));
            }
            #[cfg(target_os = "macos")]
            jumps::dock::install(app.handle().clone());
            jumps::changed(&dir.join("library.json"));
            app.manage(LibraryPath(dir.join("library.json")));
            app.manage(ssh::Shells::default());
            app.manage(Desktops::default());
            app.manage(Files::default());
            app.manage(RdpSessions::default());
            app.manage(Transfers::default());
            #[cfg(desktop)]
            {
                update::cleanup();
                app.manage(update::State::default());
                update::start(app.handle().clone());
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            key_capture,
            relabel_menu,
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
            ui_material,
            stage_file,
            folder_pick,
            folder_open,
            folder_close,
            local_import,
            transfer_pause,
            local_read,
            local_temp,
            local_open,
            immersive,
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
            local_touch,
            local_trash,
            local_list,
            save_bytes,
            certs_list,
            quick_register,
            read_key_file,
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
            rdp_close,
            update_status,
            update_set_auto,
            update_check,
            update_install
        ])
        .build(tauri::generate_context!())
        .expect("Sesam Remote konnte nicht starten")
        .run(|_app, _e| {
            // Beim Beenden ein bereits geprüftes Update still installieren (src/update.rs).
            #[cfg(desktop)]
            if let tauri::RunEvent::Exit = _e {
                update::install_on_quit(_app);
            }
            // macOS: Klick aufs Dock-Symbol bei ausgeblendetem Hauptfenster (rotes Schliessen) holt es zurück.
            #[cfg(target_os = "macos")]
            if let tauri::RunEvent::Reopen { has_visible_windows: false, .. } = _e
                && let Some(w) = _app.get_webview_window("main") {
                    let _ = w.show();
                    let _ = w.set_focus();
                }
        });
}

#[cfg(test)]
mod shortcut_tests {
    #[test]
    fn desktop_starter_ohne_eingeschleuste_zeilen() {
        let d = super::desktop_entry("Büro\nExec=rm -rf ~\r", "Shell", "sesam://open?id=abc-1&view=shell");
        assert_eq!(d, "[Desktop Entry]\nType=Application\nName=BüroExec=rm -rf ~ (Shell)\nExec=xdg-open \"sesam://open?id=abc-1&view=shell\"\nIcon=sesam-remote\nTerminal=false\n");
        assert_eq!(d.lines().filter(|l| l.starts_with("Exec=")).count(), 1);
    }
}

#[cfg(test)]
mod gateway_tests {
    #[test]
    fn gateway_adresse_lesen() {
        assert_eq!(super::gateway_addr("gw.firma.ch"), ("gw.firma.ch".into(), 443));
        assert_eq!(super::gateway_addr("gw.firma.ch:8443"), ("gw.firma.ch".into(), 8443));
        assert_eq!(super::gateway_addr("[::1]:443"), ("::1".into(), 443));
        assert_eq!(super::gateway_addr("[fe80::1]"), ("fe80::1".into(), 443));
        assert_eq!(super::gateway_addr(" 10.0.0.5:4433 "), ("10.0.0.5".into(), 4433));
    }
}

#[cfg(test)]
mod import_tests {
    use super::*;

    #[test]
    fn fremde_zuweisungen_und_reservierte_kennungen() {
        let fresh: std::collections::HashSet<String> = ["neu".to_string()].into();
        let mut c = Connection { id: "v1".into(), credential_id: Some("neu".into()), rdp_credential_id: Some("vorhanden".into()), gateway_credential_id: Some("vorhanden".into()), ..Default::default() };
        let none = std::collections::HashSet::new();
        c.agent_forward = true;
        c.jump_host = "vorhandene-verbindung".into();
        assert_eq!(import_unlink(&mut c, &fresh, &none).unwrap(), 4);
        assert!(!c.agent_forward && c.jump_host.is_empty());
        assert_eq!((c.credential_id.as_deref(), c.rdp_credential_id, c.gateway_credential_id), (Some("neu"), None, None));
        let mut bad = Connection { id: "cred-abc".into(), ..Default::default() };
        assert!(import_unlink(&mut bad, &fresh, &none).is_err());
        let mut j = Connection { id: "v2".into(), jump_host: "v1".into(), ..Default::default() };
        assert_eq!(import_unlink(&mut j, &fresh, &["v1".to_string()].into()).unwrap(), 0, "Sprung über mitgelieferte Verbindung bleibt");
        let mut q = Connection { id: "quick-123".into(), ..Default::default() };
        assert!(import_unlink(&mut q, &fresh, &none).is_err());
    }

    /// Attrappe mit der ID eines vorhandenen Sprungservers plus Verbindung, die über ihn springen will: der Verweis fällt weg.
    #[test]
    fn attrappe_mit_vorhandener_id_oeffnet_keinen_sprung() {
        let have = [Connection { id: "bastion".into(), ..Default::default() }];
        let imported = [serde_json::json!({"id": "bastion"}), serde_json::json!({"id": "neu"}), serde_json::json!({"id": "quick-1"}), serde_json::json!({"id": "cred-x"})];
        let fresh_conns = fresh_conn_ids(&imported, &have);
        assert_eq!(fresh_conns, ["neu".to_string()].into());
        let mut c = Connection { id: "neu".into(), jump_host: "bastion".into(), ..Default::default() };
        assert_eq!(import_unlink(&mut c, &Default::default(), &fresh_conns).unwrap(), 1);
        assert!(c.jump_host.is_empty());
    }
}

#[cfg(test)]
mod open_tests {
    #[test]
    fn ausfuehrbares_wird_nicht_geoeffnet() {
        #[cfg(windows)]
        let bad = ["/c/deploy.BAT", "C:\\x\\app.js", "/t/notes.txt.lnk", "/t/x.hta.", "/t/a.exe "];
        #[cfg(target_os = "macos")]
        let bad = ["/t/run.command", "/t/x.TERMINAL", "/t/Ort.fileloc", "/t/x.app/", "/t/a.pkg "];
        #[cfg(not(any(windows, target_os = "macos")))]
        let bad = ["/t/x.desktop", "/t/X.AppImage", "/t/a.jar"];
        for p in bad {
            assert!(super::runs_on_open(p), "{p}");
        }
        for p in ["/t/notes.txt", "/t/index.html", "/t/deploy.sh", "/t/config.yaml", "/t/Makefile", "/t/bild.png", "/t/bericht.docx"] {
            assert!(!super::runs_on_open(p), "{p}");
        }
    }
}

#[cfg(test)]
mod files_reconnect_tests {
    use super::*;

    fn conn() -> Connection {
        Connection { id: "c1".into(), address: "server".into(), ..Default::default() }
    }

    /// Nach einem Server-Neustart ist die SSH-Verbindung der Dateisitzung weg: die nächste Aktion (z. B. Vorschau)
    /// baut sie einmal unter derselben Kennung neu auf und läuft dann mit der neuen Sitzung.
    #[tokio::test(flavor = "multi_thread")]
    async fn tote_sitzung_wird_unter_gleicher_kennung_ersetzt() {
        let files = Files::default();
        let alt = FileSession::Sftp(Arc::new(sftp::tests::memory(&[("alt.txt", b"alt")]).await));
        files.0.lock().unwrap().insert("s1".into(), alt);
        files.1.lock().unwrap().insert("s1".into(), conn());
        let neu = Arc::new(sftp::tests::memory(&[("neu.txt", b"neu")]).await);
        let calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let (n, c2) = (neu.clone(), calls.clone());
        // Erste Abfrage: Sitzung gilt als tot → neu aufbauen.
        let s = files_live(&files, "s1", |_| true, move |c| async move {
            assert_eq!(c.id, "c1");
            c2.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Ok(FileSession::Sftp(n))
        })
        .await
        .unwrap();
        assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 1);
        let FileSession::Sftp(s) = s else { panic!("SFTP erwartet") };
        assert!(Arc::ptr_eq(&s, &neu));
        assert_eq!(s.list("/home").await.unwrap().entries.iter().map(|e| e.name.as_str()).collect::<Vec<_>>(), ["neu.txt"]);
        // Eingetragen: weitere Aktionen nehmen die neue Sitzung, ohne erneut zu verbinden.
        let s = files_live(&files, "s1", |_| false, |_| async { Err::<FileSession, String>("darf nicht verbinden".into()) }).await.unwrap();
        assert!(matches!(s, FileSession::Sftp(x) if Arc::ptr_eq(&x, &neu)));
    }

    /// Lebende Sitzungen und solche ohne gespeicherte Verbindung bleiben unberührt; ein geschlossener Tab bleibt zu.
    #[tokio::test(flavor = "multi_thread")]
    async fn nur_tote_ssh_sitzungen_werden_ersetzt() {
        let files = Files::default();
        let s = Arc::new(sftp::tests::memory(&[]).await);
        files.0.lock().unwrap().insert("lebt".into(), FileSession::Sftp(s.clone()));
        files.1.lock().unwrap().insert("lebt".into(), conn());
        let r = files_live(&files, "lebt", |_| false, |_| async { Err::<FileSession, String>("darf nicht verbinden".into()) }).await.unwrap();
        assert!(matches!(r, FileSession::Sftp(x) if Arc::ptr_eq(&x, &s)));
        // Ohne gespeicherte Verbindung (z. B. FTP): alte Sitzung zurück, der Fehler kommt vom Befehl selbst.
        files.0.lock().unwrap().insert("ohne".into(), FileSession::Sftp(s.clone()));
        let r = files_live(&files, "ohne", |_| true, |_| async { Err::<FileSession, String>("darf nicht verbinden".into()) }).await.unwrap();
        assert!(matches!(r, FileSession::Sftp(x) if Arc::ptr_eq(&x, &s)));
        // Neu aufbauen scheitert: Fehler geht an die Oberfläche (Tab bietet «Neu verbinden» an).
        let e = files_live(&files, "lebt", |_| true, |_| async { Err::<FileSession, String>("Connection refused".into()) }).await.err().unwrap();
        assert_eq!(e, "Connection refused");
        // Unbekannte Kennung: geschlossen.
        assert!(files_live(&files, "weg", |_| true, |_| async { Err::<FileSession, String>(String::new()) }).await.is_err());
    }
}

#[cfg(test)]
mod setup_script_tests {
    #[test]
    fn prozent_dekodieren() {
        assert_eq!(super::percent_decode("B%C3%BCro%20Plan.pdf"), "Büro Plan.pdf");
        assert_eq!(super::percent_decode("a%2"), "a%2");
        assert_eq!(super::percent_decode("100%"), "100%");
        assert_eq!(super::percent_decode("%zz"), "%zz");
    }

    /// Das Setup-Script wird unverändert ausgeliefert (LF, ohne BOM); die Summe muss zur freigegebenen Fassung passen.
    #[test]
    fn setup_script_is_the_released_version() {
        let (bytes, sha) = super::setup_script();
        assert_eq!(sha, "066c8bbdf4ca6173713f3e21f46d1fa695aac8d1815e388dc2fe868492571b7a");
        assert_eq!(bytes.len(), 17656);
        assert!(!bytes.starts_with(&[0xEF, 0xBB, 0xBF]) && !bytes.windows(2).any(|w| w == b"\r\n"));
    }
}

#[cfg(test)]
mod pins_tests {
    #[test]
    fn merken_und_vergessen() {
        let p = std::env::temp_dir().join(format!("sesam-pins-{}.json", std::process::id()));
        super::save_pins(&p, "a:3389", Some("AA".into())).unwrap();
        super::save_pins(&p, "b:990", Some("BB".into())).unwrap();
        super::save_pins(&p, "a:3389", None).unwrap();
        let m = super::load_pins(&p);
        assert_eq!((m.get("a:3389"), m.get("b:990").map(String::as_str)), (None, Some("BB")));
        assert!(!p.with_extension("json.tmp").exists());
        std::fs::remove_file(p).unwrap();
    }
}
