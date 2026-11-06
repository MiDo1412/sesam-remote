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

pub struct Client {
    host: String,
    port: u16,
    /// Vom Benutzer bestätigter Fingerabdruck (SHA256) für einen noch unbekannten Host.
    trusted: Option<String>,
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
        match keys::check_known_hosts(&self.host, self.port, key) {
            Ok(true) => Ok(true),
            Ok(false) => {
                let fp = key.fingerprint(keys::HashAlg::Sha256).to_string();
                if self.trusted.as_deref() != Some(fp.as_str()) {
                    *self.rejected.lock().unwrap() = Some(format!("{HOSTKEY_UNKNOWN}{}|{fp}", key.algorithm()));
                    return Ok(false);
                }
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

/// Fehler-Präfix, auf das das Frontend mit einer Rückfrage reagiert: `HOSTKEY_UNKNOWN:<Algorithmus>|<SHA256-Fingerabdruck>`.
pub const HOSTKEY_UNKNOWN: &str = "HOSTKEY_UNKNOWN:";

/// Nur ed25519: klein genug für jeden Tresor (Windows: max. 2560 Byte pro Eintrag).
pub fn check_private_key(pem: &str) -> Result<(), String> {
    let k = keys::PrivateKey::from_openssh(pem).map_err(|_| "Kein gültiger privater Schlüssel im OpenSSH-Format".to_string())?;
    if k.algorithm() != keys::Algorithm::Ed25519 {
        return Err(format!(
            "Nur ed25519-Schlüssel werden unterstützt ({} ist zu gross für den Tresor). Neuen Schlüssel erzeugen: ssh-keygen -t ed25519",
            k.algorithm()
        ));
    }
    Ok(())
}

pub struct Target<'a> {
    pub host: &'a str,
    pub port: u16,
    pub user: &'a str,
    pub password: Option<String>,
    pub key: Option<String>,
    pub trusted: Option<String>,
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

/// Verbinden, Host-Schlüssel prüfen, anmelden. Gemeinsam für Shell und SFTP.
pub async fn connect(t: &Target<'_>) -> Result<Handle<Client>, String> {
    let rejected = Arc::new(Mutex::new(None));
    let client = Client { host: t.host.into(), port: t.port, trusted: t.trusted.clone(), rejected: rejected.clone() };
    let config = Arc::new(client::Config::default());
    let mut handle = client::connect(config, (t.host, t.port), client).await.map_err(|e| {
        rejected.lock().unwrap().take().unwrap_or_else(|| format!("Verbindung fehlgeschlagen: {e}"))
    })?;
    authenticate(&mut handle, t).await?;
    Ok(handle)
}

pub async fn open(t: Target<'_>, cols: u32, rows: u32, on_event: Channel<ShellEvent>) -> Result<Shell, String> {
    let handle = connect(&t).await?;
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

#[cfg(test)]
mod tests {
    use super::check_private_key;

    // Wegwerf-Testschlüssel, nirgends verwendet.
    const ED25519: &str = "-----BEGIN OPENSSH PRIVATE KEY-----
b3BlbnNzaC1rZXktdjEAAAAABG5vbmUAAAAEbm9uZQAAAAAAAAABAAAAMwAAAAtzc2gtZW
QyNTUxOQAAACDlDxZLMtTky1GJaepWawf/6ZTZAVqZ4zqwxnxFmCZ0OQAAAJBRyIM9UciD
PQAAAAtzc2gtZWQyNTUxOQAAACDlDxZLMtTky1GJaepWawf/6ZTZAVqZ4zqwxnxFmCZ0OQ
AAAEDAbRRpxNt1PGb4ZeEvBw1crpg9Y5mqVYKHTBZweIBJ7OUPFksy1OTLUYlp6lZrB//p
lNkBWpnjOrDGfEWYJnQ5AAAACnNlc2FtLXRlc3QBAgM=
-----END OPENSSH PRIVATE KEY-----
";
    const ECDSA: &str = "-----BEGIN OPENSSH PRIVATE KEY-----
b3BlbnNzaC1rZXktdjEAAAAABG5vbmUAAAAEbm9uZQAAAAAAAAABAAAAaAAAABNlY2RzYS
1zaGEyLW5pc3RwMjU2AAAACG5pc3RwMjU2AAAAQQRKr8yoDtulBrXt8SPDr+vVtLZsTEmX
/gDNDAyvH5zXQWw/N0yi3dd2BrzKRC5Djrdn7anDvXcclM1APbmmnQM+AAAAqHb8ffd2/H
33AAAAE2VjZHNhLXNoYTItbmlzdHAyNTYAAAAIbmlzdHAyNTYAAABBBEqvzKgO26UGte3x
I8Ov69W0tmxMSZf+AM0MDK8fnNdBbD83TKLd13YGvMpELkOOt2ftqcO9dxyUzUA9uaadAz
4AAAAhAK7qO10O11s59dxFopO6hn49e9Jb8n5SZkexWdm6MOW/AAAACnNlc2FtLXRlc3QB
AgMEBQ==
-----END OPENSSH PRIVATE KEY-----
";

    #[test]
    fn accepts_only_ed25519() {
        assert!(check_private_key(ED25519).is_ok(), "{:?}", check_private_key(ED25519));
        assert!(check_private_key(ECDSA).unwrap_err().contains("Nur ed25519"));
        assert!(check_private_key("kein Schlüssel").is_err());
    }
}
