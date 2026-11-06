//! SSH-Shell über russh. Ausgabe geht über einen Tauri-Kanal ans Terminal (xterm.js).

use russh::client::{self, Handle, Msg};
use russh::keys::{self, PrivateKeyWithHashAlg, PublicKeyOrCertificate};
use russh::{ChannelMsg, ChannelWriteHalf, cipher, compression, kex, mac};
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

/// Windows speichert Geheimnisse als UTF-16 mit höchstens 2560 Byte, also 1280 Zeichen.
const VAULT_MAX_CHARS: usize = 1280;

/// ed25519 oder ECDSA im OpenSSH- oder PuTTY-Format (.ppk v2/v3), klein genug für jeden Tresor.
/// RSA passt ab 2048 Bit nicht mehr in die Windows-Anmeldeinformationsverwaltung.
pub fn check_private_key(text: &str) -> Result<(), String> {
    let t = text.trim();
    let algorithm = if let Some(rest) = t.strip_prefix("PuTTY-User-Key-File-") {
        // Kopfzeile «PuTTY-User-Key-File-3: ssh-ed25519». Verschlüsselte Dateien lassen sich ohne
        // Passphrase nicht ganz prüfen; das geschieht beim Verbinden (Passphrase = Passwort).
        let (version, alg) = rest.lines().next().and_then(|l| l.split_once(": ")).ok_or("Ungültige PuTTY-Schlüsseldatei")?;
        if !["2", "3"].contains(&version) {
            return Err(format!("PuTTY-Schlüsseldatei Version {version} wird nicht unterstützt (nur 2 und 3)"));
        }
        if t.contains("\nEncryption: none") {
            keys::PrivateKey::from_ppk(t, None).map_err(|e| format!("PuTTY-Schlüsseldatei nicht lesbar: {e}"))?;
        }
        alg.parse::<keys::Algorithm>().map_err(|_| format!("Unbekannter Schlüsseltyp {alg}"))?
    } else {
        keys::PrivateKey::from_openssh(t)
            .map_err(|_| "Kein gültiger privater Schlüssel (OpenSSH-Format oder PuTTY .ppk)".to_string())?
            .algorithm()
    };
    if !matches!(algorithm, keys::Algorithm::Ed25519 | keys::Algorithm::Ecdsa { .. }) {
        return Err(format!(
            "Nur ed25519- und ECDSA-Schlüssel werden unterstützt ({algorithm} ist zu gross für den Tresor). Neuen Schlüssel erzeugen: ssh-keygen -t ed25519"
        ));
    }
    if text.encode_utf16().count() > VAULT_MAX_CHARS {
        return Err("Schlüssel ist zu gross für den Tresor (höchstens 1280 Zeichen)".into());
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
    pub opts: Options,
}

/// Verbindungsoptionen pro Eintrag der Bibliothek.
#[derive(Clone, Copy, Default, Debug)]
pub struct Options {
    /// Veraltete Algorithmen zusätzlich anbieten (SHA-1, CBC, 3DES, ssh-rsa), am Ende der Liste.
    pub legacy: bool,
    /// Sekunden zwischen Keepalives, 0 = aus.
    pub keepalive: u32,
    pub compression: bool,
}

const KEX: &[kex::Name] = &[
    kex::MLKEM768X25519_SHA256,
    kex::CURVE25519,
    kex::CURVE25519_PRE_RFC_8731,
    kex::ECDH_SHA2_NISTP256,
    kex::ECDH_SHA2_NISTP384,
    kex::ECDH_SHA2_NISTP521,
    kex::DH_GEX_SHA256,
    kex::DH_G16_SHA512,
    kex::DH_G18_SHA512,
    kex::DH_G14_SHA256,
];
const KEX_LEGACY: &[kex::Name] = &[kex::DH_G14_SHA1, kex::DH_GEX_SHA1, kex::DH_G1_SHA1];
/// Keine Verfahren, sondern Signale (Erweiterungen, «strict kex» gegen Terrapin); gehören immer dazu.
const KEX_EXT: &[kex::Name] = &[kex::EXTENSION_SUPPORT_AS_CLIENT, kex::EXTENSION_OPENSSH_STRICT_KEX_AS_CLIENT];
const CIPHERS: &[cipher::Name] =
    &[cipher::CHACHA20_POLY1305, cipher::AES_256_GCM, cipher::AES_128_GCM, cipher::AES_256_CTR, cipher::AES_192_CTR, cipher::AES_128_CTR];
const CIPHERS_LEGACY: &[cipher::Name] = &[cipher::AES_256_CBC, cipher::AES_192_CBC, cipher::AES_128_CBC, cipher::TRIPLE_DES_CBC];
const MACS: &[mac::Name] = &[mac::HMAC_SHA512_ETM, mac::HMAC_SHA256_ETM, mac::HMAC_SHA512, mac::HMAC_SHA256];
const MACS_LEGACY: &[mac::Name] = &[mac::HMAC_SHA1_ETM, mac::HMAC_SHA1];

/// Algorithmen-Reihenfolge (modern zuerst) und Keepalive. Nur was russh selbst implementiert.
pub fn config(o: Options) -> client::Config {
    use keys::{Algorithm, EcdsaCurve, HashAlg};
    fn pick<T: Clone>(modern: &[T], legacy: &[T], on: bool) -> Vec<T> {
        [modern, if on { legacy } else { &[] }].concat()
    }
    let mut key = vec![
        Algorithm::Ed25519,
        Algorithm::Ecdsa { curve: EcdsaCurve::NistP256 },
        Algorithm::Ecdsa { curve: EcdsaCurve::NistP384 },
        Algorithm::Ecdsa { curve: EcdsaCurve::NistP521 },
        Algorithm::Rsa { hash: Some(HashAlg::Sha512) },
        Algorithm::Rsa { hash: Some(HashAlg::Sha256) },
    ];
    if o.legacy {
        key.push(Algorithm::Rsa { hash: None }); // ssh-rsa mit SHA-1
    }
    let compression =
        if o.compression { vec![compression::ZLIB_LEGACY, compression::ZLIB, compression::NONE] } else { vec![compression::NONE] };
    client::Config {
        preferred: russh::Preferred {
            kex: [pick(KEX, KEX_LEGACY, o.legacy), KEX_EXT.to_vec()].concat().into(),
            key: key.into(),
            cipher: pick(CIPHERS, CIPHERS_LEGACY, o.legacy).into(),
            mac: pick(MACS, MACS_LEGACY, o.legacy).into(),
            compression: compression.into(),
            ..Default::default()
        },
        keepalive_interval: (o.keepalive > 0).then(|| std::time::Duration::from_secs(o.keepalive.into())),
        ..Default::default()
    }
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
    let config = Arc::new(config(t.opts));
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
    use super::*;

    #[test]
    fn algorithm_lists() {
        let names = |c: &client::Config| {
            let p = &c.preferred;
            let mut v: Vec<String> = p.kex.iter().map(|n| n.as_ref().to_string()).collect();
            v.extend(p.cipher.iter().map(|n| n.as_ref().to_string()));
            v.extend(p.mac.iter().map(|n| n.as_ref().to_string()));
            v.extend(p.key.iter().map(|k| format!("{k:?}")));
            v
        };
        let old = ["diffie-hellman-group14-sha1", "aes128-cbc", "3des-cbc", "hmac-sha1", "Rsa { hash: None }"];
        let modern = config(Options::default());
        let m = names(&modern);
        assert_eq!(m[0], "mlkem768x25519-sha256");
        for o in old {
            assert!(!m.iter().any(|n| n == o), "{o} gehört nicht in die Standardliste");
        }
        assert!(m.iter().any(|n| n == "kex-strict-c-v00@openssh.com"));
        assert_eq!(modern.preferred.compression.iter().map(|c| c.as_ref()).collect::<Vec<_>>(), ["none"]);
        assert!(modern.keepalive_interval.is_none());

        let legacy = config(Options { legacy: true, keepalive: 30, compression: true });
        let l = names(&legacy);
        for o in old {
            assert!(l.iter().any(|n| n == o), "{o} fehlt");
        }
        assert_eq!(l[0], "mlkem768x25519-sha256", "Veraltetes nur hinten");
        assert_eq!(legacy.preferred.compression[0].as_ref(), "zlib@openssh.com");
        assert_eq!(legacy.keepalive_interval, Some(std::time::Duration::from_secs(30)));
    }

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

    // PuTTY-Testschlüssel aus den Beispielen von `ssh-key` (Passphrase «123»).
    const PPK_ED25519: &str = "PuTTY-User-Key-File-3: ssh-ed25519
Encryption: none
Comment: user@example.com
Public-Lines: 2
AAAAC3NzaC1lZDI1NTE5AAAAILM+rvN+ot98qgEN796jTiQfZfG1KaT0PtFDJ/XF
Sqti
Private-Lines: 1
AAAAILYGwiLRDBba4WxwpNRRc0cuxhfgXGVpINJuVsCPtZHt
Private-MAC: 94140d0344fad6aa1bf7b71e9c93db11ccac8a232f8a51e11c024869d608c82d
";
    const PPK_ED25519_ENC: &str = "PuTTY-User-Key-File-3: ssh-ed25519
Encryption: aes256-cbc
Comment: user@example.com
Public-Lines: 2
AAAAC3NzaC1lZDI1NTE5AAAAILM+rvN+ot98qgEN796jTiQfZfG1KaT0PtFDJ/XF
Sqti
Key-Derivation: Argon2id
Argon2-Memory: 8192
Argon2-Passes: 34
Argon2-Parallelism: 1
Argon2-Salt: 63d1d43f7bf7700720496646a2f5ec17
Private-Lines: 1
DyWtExZ3dxFutnb12tIwXBC6kWdozrvP+r6faHKBGDb4+qEar9XBiC0BmGySMHUi
Private-MAC: 52fd00d4ef47ebc506e4e709486c0c6bc0606e24fe2c6cb1b3d168f4da238a66
";
    const PPK_ECDSA: &str = "PuTTY-User-Key-File-3: ecdsa-sha2-nistp256
Encryption: none
Comment: user@example.com
Public-Lines: 3
AAAAE2VjZHNhLXNoYTItbmlzdHAyNTYAAAAIbmlzdHAyNTYAAABBBHwf2HMM5TRX
vo2SQJjsNkiDD5KqiiNjrGVv3UUh+mMT5RHxiRtOnlqvjhQtBq0VpmpCV/PwUdhO
ig4vkbqAcEc=
Private-Lines: 1
AAAAIQDKeKZHdL+uNxIyJJN/A5iWAYlwesoKhkXOtDWcQjugeQ==
Private-MAC: 4df1d2227cb63bbbaf0821816cb82afffc2c97163d22afb114f68621a55b07c7
";

    #[test]
    fn accepted_keys() {
        for k in [ED25519, ECDSA, PPK_ED25519, PPK_ED25519_ENC, PPK_ECDSA] {
            assert!(check_private_key(k).is_ok(), "{:?}", check_private_key(k));
        }
        let rsa = "PuTTY-User-Key-File-3: ssh-rsa\nEncryption: aes256-cbc\n";
        assert!(check_private_key(rsa).unwrap_err().contains("Nur ed25519- und ECDSA"));
        assert!(check_private_key("PuTTY-User-Key-File-1: ssh-ed25519\n").unwrap_err().contains("Version 1"));
        let broken = PPK_ED25519.replace("Private-MAC: 9", "Private-MAC: 8");
        assert!(check_private_key(&broken).is_err());
        assert!(check_private_key("kein Schlüssel").is_err());
        assert!(check_private_key(&format!("{ED25519}{}", " ".repeat(1300))).unwrap_err().contains("zu gross"));
    }

    #[test]
    fn ppk_decodes_for_login() {
        // So liest `authenticate` den Schlüssel: erst ohne, dann mit Passwort als Passphrase.
        assert_eq!(keys::decode_secret_key(PPK_ED25519, None).unwrap().algorithm(), keys::Algorithm::Ed25519);
        assert!(keys::decode_secret_key(PPK_ED25519_ENC, None).is_err());
        assert_eq!(keys::decode_secret_key(PPK_ED25519_ENC, Some("123")).unwrap().algorithm(), keys::Algorithm::Ed25519);
        assert!(keys::decode_secret_key(PPK_ECDSA, None).unwrap().algorithm().is_ecdsa());
    }
}
