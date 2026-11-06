//! SSH-Shell über russh. Ausgabe geht über einen Tauri-Kanal ans Terminal (xterm.js).

use russh::client::{self, Handle, Msg};
use russh::keys::{self, PrivateKeyWithHashAlg, PublicKeyOrCertificate};
use russh::{ChannelMsg, ChannelWriteHalf};
use serde::Serialize;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tauri::ipc::Channel;

#[derive(Clone, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ShellEvent {
    Data { data: Vec<u8> },
    Closed { reason: String },
}

pub struct Shell {
    handle: Handle<Client>,
    writer: ChannelWriteHalf<Msg>,
}

#[derive(Default)]
pub struct Shells(pub tokio::sync::Mutex<HashMap<String, Shell>>);

struct Client {
    host: String,
    port: u16,
    /// Grund für eine Ablehnung des Server-Schlüssels, damit die Meldung verständlich ist.
    rejected: Arc<Mutex<Option<String>>>,
}

impl client::Handler for Client {
    type Error = russh::Error;

    async fn check_server_key(&mut self, key: &PublicKeyOrCertificate) -> Result<bool, Self::Error> {
        let PublicKeyOrCertificate::PublicKey { key, .. } = key else {
            *self.rejected.lock().unwrap() = Some("Server-Zertifikate werden noch nicht unterstützt".into());
            return Ok(false);
        };
        // shortcut: unbekannte Hosts werden beim ersten Mal ohne Rückfrage gelernt (wie OpenSSH «accept-new»); später Fingerabdruck im UI bestätigen lassen.
        match keys::check_known_hosts(&self.host, self.port, key) {
            Ok(true) => Ok(true),
            Ok(false) => {
                if let Err(e) = keys::known_hosts::learn_known_hosts(&self.host, self.port, key) {
                    *self.rejected.lock().unwrap() = Some(format!("known_hosts nicht schreibbar: {e}"));
                    return Ok(false);
                }
                Ok(true)
            }
            Err(keys::Error::KeyChanged { line }) => {
                *self.rejected.lock().unwrap() = Some(format!(
                    "Der Server-Schlüssel hat sich geändert (known_hosts, Zeile {line}). Verbindung abgebrochen."
                ));
                Ok(false)
            }
            Err(e) => {
                *self.rejected.lock().unwrap() = Some(format!("known_hosts: {e}"));
                Ok(false)
            }
        }
    }
}

pub struct Target<'a> {
    pub host: &'a str,
    pub port: u16,
    pub user: &'a str,
    pub password: Option<String>,
    pub key: Option<String>,
}

async fn authenticate(h: &mut Handle<Client>, t: &Target<'_>) -> Result<(), String> {
    if let Some(pem) = &t.key {
        let pk = keys::decode_secret_key(pem, None)
            .or_else(|_| keys::decode_secret_key(pem, t.password.as_deref()))
            .map_err(|e| format!("SSH-Schlüssel nicht lesbar: {e}"))?;
        let hash = h.best_supported_rsa_hash().await.map_err(|e| e.to_string())?.flatten();
        let r = h
            .authenticate_publickey(t.user, PrivateKeyWithHashAlg::new(Arc::new(pk), hash))
            .await
            .map_err(|e| e.to_string())?;
        if r.success() {
            return Ok(());
        }
    }
    if let Some(pw) = &t.password {
        let r = h.authenticate_password(t.user, pw).await.map_err(|e| e.to_string())?;
        if r.success() {
            return Ok(());
        }
    }
    Err("Anmeldung abgelehnt".into())
}

pub async fn open(t: Target<'_>, cols: u32, rows: u32, on_event: Channel<ShellEvent>) -> Result<Shell, String> {
    let rejected = Arc::new(Mutex::new(None));
    let client = Client { host: t.host.into(), port: t.port, rejected: rejected.clone() };
    let config = Arc::new(client::Config::default());
    let mut handle = client::connect(config, (t.host, t.port), client).await.map_err(|e| {
        rejected.lock().unwrap().take().unwrap_or_else(|| format!("Verbindung fehlgeschlagen: {e}"))
    })?;
    authenticate(&mut handle, &t).await?;

    let channel = handle.channel_open_session().await.map_err(|e| e.to_string())?;
    channel.request_pty(false, "xterm-256color", cols, rows, 0, 0, &[]).await.map_err(|e| e.to_string())?;
    channel.request_shell(false).await.map_err(|e| e.to_string())?;
    let (mut reader, writer) = channel.split();

    tauri::async_runtime::spawn(async move {
        let mut reason = "Verbindung beendet".to_string();
        while let Some(msg) = reader.wait().await {
            match msg {
                ChannelMsg::Data { data } | ChannelMsg::ExtendedData { data, .. } => {
                    if on_event.send(ShellEvent::Data { data: data.to_vec() }).is_err() {
                        return; // Fenster geschlossen
                    }
                }
                ChannelMsg::ExitStatus { exit_status } => reason = format!("Shell beendet (Code {exit_status})"),
                _ => {}
            }
        }
        let _ = on_event.send(ShellEvent::Closed { reason });
    });

    Ok(Shell { handle, writer })
}

impl Shell {
    pub async fn write(&self, data: Vec<u8>) -> Result<(), String> {
        self.writer.data_bytes(data).await.map_err(|e| e.to_string())
    }

    pub async fn resize(&self, cols: u32, rows: u32) -> Result<(), String> {
        self.writer.window_change(cols, rows, 0, 0).await.map_err(|e| e.to_string())
    }

    pub async fn close(self) {
        let _ = self.writer.close().await;
        let _ = self.handle.disconnect(russh::Disconnect::ByApplication, "", "de").await;
    }
}
