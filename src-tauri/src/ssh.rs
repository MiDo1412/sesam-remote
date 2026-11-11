//! SSH-Shell über russh. Ausgabe geht über einen Tauri-Kanal ans Terminal (xterm.js).

use russh::client::{self, Handle, Msg};
use russh::keys::{self, PrivateKeyWithHashAlg, PublicKeyOrCertificate};
use crate::forward;
use russh::{ChannelMsg, ChannelWriteHalf, cipher, compression, kex, mac};
use serde::Serialize;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

#[derive(Clone, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ShellEvent {
    Data { data: Vec<u8> },
    Closed { reason: String },
}

/// Befehle für `Shell::probe`: nur Lesen, vcgencmd nur wenn vorhanden (Raspberry Pi OS).
const PROBE: &str = "printf 'model='; tr -d '\\0' < /proc/device-tree/model 2>/dev/null; echo; (. /etc/os-release 2>/dev/null && echo \"id=$ID\"); \
command -v vcgencmd >/dev/null 2>&1 && { vcgencmd measure_temp; vcgencmd get_throttled; }";

#[derive(serde::Serialize, Default, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Probe {
    /// z. B. «Raspberry Pi 5 Model B Rev 1.0», leer ohne Device-Tree.
    pub model: String,
    /// ID aus /etc/os-release (debian, raspbian, ubuntu …).
    pub os_id: String,
    /// Grad Celsius (nur mit vcgencmd).
    pub temp: Option<f32>,
    /// Unterspannung gerade jetzt bzw. seit dem Start (get_throttled Bit 0 bzw. 16).
    pub undervolt_now: bool,
    pub undervolt_since_boot: bool,
}

/// Rechner kurz befragen (eigener exec-Kanal derselben Verbindung): Modell, System, Temperatur, Unterspannung.
pub async fn probe(h: &Handle<Client>) -> Result<Probe, String> {
    let o = crate::scp::Remote::run(h, PROBE.into()).await?;
    Ok(parse_probe(&String::from_utf8_lossy(&o.out)))
}

pub fn parse_probe(out: &str) -> Probe {
    let mut p = Probe::default();
    for l in out.lines().map(str::trim) {
        if let Some(v) = l.strip_prefix("model=") {
            p.model = v.trim().to_string();
        } else if let Some(v) = l.strip_prefix("id=") {
            p.os_id = v.trim_matches('"').to_string();
        } else if let Some(v) = l.strip_prefix("temp=") {
            p.temp = v.trim_end_matches("'C").parse().ok();
        } else if let Some(v) = l.strip_prefix("throttled=0x") {
            let bits = u32::from_str_radix(v, 16).unwrap_or(0);
            p.undervolt_now = bits & 1 != 0;
            p.undervolt_since_boot = bits & (1 << 16) != 0;
        }
    }
    p
}

pub struct Shell {
    handle: Arc<Handle<Client>>,
    writer: ChannelWriteHalf<Msg>,
    /// Portweiterleitungen leben so lange wie die Shell.
    _forwards: forward::Forwards,
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
    /// Fernweiterleitungen (R): Port auf dem Server → Ziel hier.
    remote: HashMap<u32, (String, u16)>,
    /// Agent-Kanäle vom Server nur annehmen, wenn wir die Weiterleitung selbst verlangt haben.
    agent_forward: bool,
    /// Verbindung zum Sprungserver; lebt so lange wie diese Sitzung.
    jump: Option<Handle<Client>>,
}

impl client::Handler for Client {
    type Error = russh::Error;

    async fn server_channel_open_agent_forward(
        &mut self,
        channel: russh::Channel<Msg>,
        reply: client::ChannelOpenHandle,
        _session: &mut client::Session,
    ) -> Result<(), Self::Error> {
        if !self.agent_forward {
            return Ok(()); // Drop von `reply` lehnt ab
        }
        reply.accept().await;
        tokio::spawn(async move {
            if let Ok(a) = agent().await {
                let _ = tokio::io::copy_bidirectional(&mut channel.into_stream(), &mut a.into_inner()).await;
            }
        });
        Ok(())
    }

    async fn server_channel_open_forwarded_tcpip(
        &mut self,
        channel: russh::Channel<Msg>,
        _connected_address: &str,
        connected_port: u32,
        _originator_address: &str,
        _originator_port: u32,
        reply: client::ChannelOpenHandle,
        _session: &mut client::Session,
    ) -> Result<(), Self::Error> {
        // Nur Ports, die wir selbst angemeldet haben; alles andere lehnt der Drop von `reply` ab.
        if let Some((host, port)) = self.remote.get(&connected_port).cloned() {
            reply.accept().await;
            forward::serve_remote(channel, host, port);
        }
        Ok(())
    }

    async fn check_server_key(&mut self, key: &PublicKeyOrCertificate) -> Result<bool, Self::Error> {
        let PublicKeyOrCertificate::PublicKey { key, .. } = key else {
            *self.rejected.lock().unwrap() = Some("Server-Zertifikate werden noch nicht unterstützt".into());
            return Ok(false);
        };
        let file = match known_hosts_file() {
            Ok(f) => f,
            Err(e) => {
                *self.rejected.lock().unwrap() = Some(e);
                return Ok(false);
            }
        };
        match keys::known_hosts::check_known_hosts_path(&self.host, self.port, key, &file) {
            Ok(true) => Ok(true),
            Ok(false) => {
                // Ist für diesen Host schon ein Schlüssel eines anderen Typs bekannt, sagt die Rückfrage das deutlich
                // (dritter Teil «anders»): sonst sähe ein Angreifer mit anderem Schlüsseltyp aus wie ein neuer Host
                // (security-review-2026-11-11, S-09). Kein harter Abbruch: ältere known_hosts kennen oft nur RSA/ECDSA.
                let other = keys::known_hosts::known_host_keys_path(&self.host, self.port, &file).is_ok_and(|v| !v.is_empty());
                let fp = key.fingerprint(keys::HashAlg::Sha256).to_string();
                if self.trusted.as_deref() != Some(fp.as_str()) {
                    *self.rejected.lock().unwrap() = Some(format!("{HOSTKEY_UNKNOWN}{}|{fp}{}", key.algorithm(), if other { "|anders" } else { "" }));
                    return Ok(false);
                }
                if let Err(e) = keys::known_hosts::learn_known_hosts_path(&self.host, self.port, key, &file) {
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

/// Desktop: ~/.ssh/known_hosts wie OpenSSH. iPad: Der Ordner der App selbst ist nicht beschreibbar (~/.ssh lässt sich
/// nicht anlegen, darum scheiterte dort jede erste Verbindung); known_hosts liegt neben der Bibliothek.
fn known_hosts_file() -> Result<std::path::PathBuf, String> {
    #[cfg(mobile)]
    if let Some(dir) = crate::LIBRARY_FILE.get().and_then(|p| p.parent()) {
        return Ok(dir.join("known_hosts"));
    }
    std::env::home_dir().map(|h| h.join(".ssh").join("known_hosts")).ok_or_else(|| "Kein Benutzerordner für known_hosts".into())
}

/// Fehler-Präfix, auf das das Frontend mit einer Rückfrage reagiert: `HOSTKEY_UNKNOWN:<Algorithmus>|<SHA256-Fingerabdruck>`.
pub const HOSTKEY_UNKNOWN: &str = "HOSTKEY_UNKNOWN:";

/// Windows speichert Geheimnisse als UTF-16 mit höchstens 2560 Byte, also 1280 Zeichen.
const VAULT_MAX_CHARS: usize = 1280;

/// ed25519 oder ECDSA im OpenSSH- oder PuTTY-Format (.ppk v2/v3), klein genug für jeden Tresor.
/// RSA passt ab 2048 Bit nicht mehr in die Windows-Anmeldeinformationsverwaltung.
/// Neuer Ed25519-Schlüssel im OpenSSH-Format (privat) und der öffentliche Schlüssel als eine Zeile.
pub fn generate_ed25519(comment: &str) -> Result<(String, String), String> {
    let mut seed = [0u8; 32];
    getrandom::fill(&mut seed).map_err(|e| format!("Zufall nicht verfügbar: {e}"))?;
    let mut k = keys::PrivateKey::from(keys::ssh_key::private::Ed25519Keypair::from_seed(&seed));
    seed.fill(0);
    k.set_comment(comment);
    let private = k.to_openssh(keys::ssh_key::LineEnding::LF).map_err(|e| e.to_string())?.to_string();
    let public = k.public_key().to_openssh().map_err(|e| e.to_string())?;
    Ok((private, public))
}

/// Öffentlicher Schlüssel (eine Zeile wie in authorized_keys) zu einem gespeicherten privaten Schlüssel.
pub fn public_key_of(private: &str) -> Result<String, String> {
    let t = private.trim();
    let k = if t.starts_with("PuTTY-User-Key-File-") { keys::PrivateKey::from_ppk(t, None) } else { keys::PrivateKey::from_openssh(t) }
        .map_err(|e| format!("Schlüssel nicht lesbar (mit Passphrase geschützt?): {e}"))?;
    k.public_key().to_openssh().map_err(|e| e.to_string())
}

/// Skript wie ssh-copy-id: öffentlichen Schlüssel an ~/.ssh/authorized_keys anhängen, wenn er dort noch fehlt.
pub fn install_key_script(public: &str) -> Result<String, String> {
    let line = public.trim();
    if line.contains(['\n', '\r', '\'']) || !(line.starts_with("ssh-") || line.starts_with("ecdsa-")) {
        return Err("Ungültiger öffentlicher Schlüssel".into());
    }
    Ok(format!(
        "umask 077; mkdir -p ~/.ssh && touch ~/.ssh/authorized_keys && (grep -qxF '{line}' ~/.ssh/authorized_keys && echo vorhanden || (echo '{line}' >> ~/.ssh/authorized_keys && echo eingetragen))"
    ))
}

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

pub struct Target {
    pub host: String,
    pub port: u16,
    pub user: String,
    pub password: Option<String>,
    pub key: Option<String>,
    pub trusted: Option<String>,
    pub opts: Options,
    /// Nur für die Shell: Portweiterleitungen.
    pub forwards: Vec<forward::Rule>,
    /// Sprungserver (ProxyJump): erst dorthin, dann durch ihn weiter (eine Stufe).
    pub jump: Option<Box<Target>>,
}

/// Verbindungsoptionen pro Eintrag der Bibliothek.
#[derive(Clone, Copy, Default, Debug)]
pub struct Options {
    /// Veraltete Algorithmen zusätzlich anbieten (SHA-1, CBC, 3DES, ssh-rsa), am Ende der Liste.
    pub legacy: bool,
    /// Sekunden zwischen Keepalives, 0 = aus.
    pub keepalive: u32,
    pub compression: bool,
    /// Zuerst mit den Schlüsseln aus Pageant bzw. dem Windows-OpenSSH-Agenten anmelden.
    pub agent: bool,
    /// Agenten an den Server weiterreichen (nur Shell).
    pub agent_forward: bool,
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

type Agent = keys::agent::client::AgentClient<Box<dyn keys::agent::client::AgentStream + Send + Unpin>>;

/// Laufender SSH-Agent: Pageant, sonst der OpenSSH-Agent von Windows; unter macOS/Linux `SSH_AUTH_SOCK`.
async fn agent() -> Result<Agent, String> {
    use keys::agent::client::AgentClient;
    #[cfg(windows)]
    let a = match AgentClient::connect_pageant().await {
        Ok(a) => Ok(a.dynamic()),
        Err(_) => AgentClient::connect_named_pipe(r"\\.\pipe\openssh-ssh-agent").await.map(|a| a.dynamic()),
    };
    #[cfg(unix)]
    let a = AgentClient::connect_env().await.map(|a| a.dynamic());
    a.map_err(|e| format!("Kein SSH-Agent erreichbar (Pageant oder OpenSSH-Agent): {e}"))
}

/// Alle Schlüssel des Agenten durchprobieren. `false`, wenn keiner passt oder kein Agent läuft.
async fn agent_auth(h: &mut Handle<Client>, user: &str) -> bool {
    let mut a = match agent().await {
        Ok(a) => a,
        Err(e) => {
            tracing::info!("{e}");
            return false;
        }
    };
    let Ok(ids) = a.request_identities().await else { return false };
    let hash = h.best_supported_rsa_hash().await.ok().flatten().flatten();
    for id in ids {
        let keys::agent::AgentIdentity::PublicKey { key, .. } = id else { continue };
        if let Ok(r) = h.authenticate_publickey_with(user, key, hash, &mut a).await
            && r.success()
        {
            return true;
        }
    }
    false
}

async fn authenticate(h: &mut Handle<Client>, t: &Target) -> Result<(), String> {
    let mut tried = Vec::new();
    if t.opts.agent {
        if agent_auth(h, &t.user).await {
            return Ok(());
        }
        tried.push("SSH-Agent");
    }
    if let Some(pem) = &t.key {
        tried.push("Schlüssel");
        let pk = keys::decode_secret_key(pem, None)
            .or_else(|_| keys::decode_secret_key(pem, t.password.as_deref()))
            .map_err(|e| format!("SSH-Schlüssel nicht lesbar: {e}"))?;
        let hash = h.best_supported_rsa_hash().await.map_err(|e| e.to_string())?.flatten();
        let r = h
            .authenticate_publickey(&t.user, PrivateKeyWithHashAlg::new(Arc::new(pk), hash))
            .await
            .map_err(|e| e.to_string())?;
        if r.success() {
            return Ok(());
        }
    }
    if let Some(pw) = &t.password {
        tried.push("Passwort");
        let r = h.authenticate_password(&t.user, pw).await.map_err(|e| e.to_string())?;
        if r.success() {
            return Ok(());
        }
    }
    tracing::info!(user = %t.user, tried = ?tried, "SSH-Anmeldung abgelehnt");
    Err(if tried.is_empty() {
        format!("Anmeldung abgelehnt: Für {} ist weder Passwort noch Schlüssel gespeichert.", t.user)
    } else {
        format!("Anmeldung als {} abgelehnt (versucht: {}).", t.user, tried.join(", "))
    })
}

/// Verbinden (direkt oder über den Sprungserver), Host-Schlüssel prüfen, anmelden. Gemeinsam für Shell und Dateien.
pub async fn connect(t: &Target) -> Result<Handle<Client>, String> {
    let rejected = Arc::new(Mutex::new(None));
    let remote = t
        .forwards
        .iter()
        .filter_map(|r| match r {
            forward::Rule::Remote { listen, host, port } => Some((u32::from(*listen), (host.clone(), *port))),
            _ => None,
        })
        .collect();
    let mut client = Client {
        host: t.host.clone(),
        port: t.port,
        trusted: t.trusted.clone(),
        rejected: rejected.clone(),
        remote,
        agent_forward: t.opts.agent && t.opts.agent_forward,
        jump: None,
    };
    let config = Arc::new(config(t.opts));
    let fail = |e: russh::Error| rejected.lock().unwrap().take().unwrap_or_else(|| format!("Verbindung fehlgeschlagen: {e}"));
    let mut handle = match &t.jump {
        None => client::connect(config, (t.host.as_str(), t.port), client).await.map_err(fail)?,
        Some(j) => {
            let jh = Box::pin(connect(j)).await.map_err(|e| {
                if e.starts_with(HOSTKEY_UNKNOWN) {
                    format!("Sprungserver {}: Server-Schlüssel noch nicht bestätigt. Bitte einmal direkt verbinden (Shell) und bestätigen.", j.host)
                } else {
                    format!("Sprungserver {}: {e}", j.host)
                }
            })?;
            let ch = jh
                .channel_open_direct_tcpip(t.host.as_str(), t.port.into(), "127.0.0.1", 0)
                .await
                .map_err(|e| format!("Sprungserver {} erreicht {}:{} nicht: {e}", j.host, t.host, t.port))?;
            client.jump = Some(jh);
            client::connect_stream(config, ch.into_stream(), client).await.map_err(fail)?
        }
    };
    authenticate(&mut handle, t).await?;
    Ok(handle)
}

/// Einen Befehl ausführen und die Ausgabe zurückgeben (nur für einfache, lesende Abfragen).
pub async fn exec(t: &Target, cmd: &str) -> Result<String, String> {
    let h = connect(t).await?;
    let mut ch = h.channel_open_session().await.map_err(|e| e.to_string())?;
    ch.exec(true, cmd.as_bytes()).await.map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    let mut status = 0;
    while let Some(m) = ch.wait().await {
        match m {
            ChannelMsg::Data { data } => out.extend_from_slice(&data),
            ChannelMsg::ExitStatus { exit_status } => status = exit_status,
            ChannelMsg::Close => break,
            _ => {}
        }
    }
    let _ = h.disconnect(russh::Disconnect::ByApplication, "", "de").await;
    if status != 0 {
        return Err(format!("Befehl fehlgeschlagen (Code {status})"));
    }
    Ok(String::from_utf8_lossy(&out).into_owned())
}

/// Zeilen «benutzer port» aus /etc/sesam-rdp/ports (Setup-Script); Unbrauchbares wird übersprungen.
pub fn parse_ports(text: &str) -> Vec<(String, u16)> {
    text.lines()
        .filter_map(|l| {
            let mut it = l.split_whitespace();
            let (u, p) = (it.next()?, it.next()?.parse().ok()?);
            (it.next().is_none() && u.chars().all(|c| c.is_ascii_alphanumeric() || "._-".contains(c))).then(|| (u.to_string(), p))
        })
        .collect()
}

/// Befehl für eine gehaltene Shell: in der tmux-Sitzung `name` (anhängen oder neu), so eingestellt, dass sie sich
/// wie ein normales Terminal anfühlt: keine Statuszeile, kein Esc-Verzug, Kopieren landet per OSC 52 in der
/// Zwischenablage, Mausrad scrollt, kein Präfix-Kürzel. Reihenfolge mit Absicht: Schlägt bei altem tmux eine
/// Option fehl (mouse ab 2.1, prefix None ab 2.2), bricht tmux nur die folgenden ab, die wichtigen stehen vorne.
/// Fehlt tmux: Hinweis und eine normale Login-Shell.
pub fn tmux_command(name: &str) -> Result<String, String> {
    if name.is_empty() || name.len() > 64 || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        return Err("Ungültiger Sitzungsname".into());
    }
    Ok(format!(
        "if command -v tmux >/dev/null 2>&1; then exec tmux new-session -A -s {name} \\; set-option status off \\; set-option -s escape-time 0 \\; set-option -s set-clipboard on \\; set-option mouse on \\; set-option prefix None; \
else printf '\\033[2m[Sesam] tmux fehlt auf diesem Server; die Shell läuft ohne Halten weiter.\\033[0m\\r\\n'; exec \"${{SHELL:-/bin/sh}}\" -l; fi"
    ))
}

/// Gehaltene Sesam-Sitzungen (tmux, Name beginnt mit «sesam-»): Name, angehängt ja/nein, gestartet (Unix-Zeit).
pub async fn tmux_list(h: &Handle<Client>) -> Result<Vec<(String, bool, u64)>, String> {
    let o = crate::scp::Remote::run(h, "tmux ls -F '#{session_name} #{session_attached} #{session_created}' 2>/dev/null".into()).await?;
    Ok(String::from_utf8_lossy(&o.out)
        .lines()
        .filter_map(|l| {
            let mut it = l.split(' ');
            let (n, a, c) = (it.next()?, it.next()?, it.next()?);
            n.starts_with("sesam-").then(|| (n.to_string(), a != "0", c.parse().unwrap_or(0)))
        })
        .collect())
}

pub async fn tmux_kill(h: &Handle<Client>, name: &str) -> Result<(), String> {
    tmux_command(name)?; // prüft den Namen
    if !name.starts_with("sesam-") {
        return Err("Nur Sitzungen von Sesam".into());
    }
    let o = crate::scp::Remote::run(h, format!("tmux kill-session -t {name}")).await?;
    if o.code == 0 { Ok(()) } else { Err(String::from_utf8_lossy(&o.err).trim().to_string()) }
}

/// Wie `open`, mit `hold` = Name einer tmux-Sitzung, in der die Shell läuft (übersteht Unterbrüche).
pub async fn open_with(t: Target, cols: u32, rows: u32, on_event: crate::relay::Relay<ShellEvent>, hold: Option<&str>) -> Result<Shell, String> {
    let handle = Arc::new(connect(&t).await?);
    let note = on_event.clone();
    let forwards = forward::start(&handle, &t.forwards, move |m| {
        let _ = note.send(ShellEvent::Data { data: format!("\x1b[2m[Sesam] {m}\x1b[0m\r\n").into_bytes() });
    })
    .await;
    let channel = handle.channel_open_session().await.map_err(|e| e.to_string())?;
    channel.request_pty(false, "xterm-256color", cols, rows, 0, 0, &[]).await.map_err(|e| e.to_string())?;
    if t.opts.agent && t.opts.agent_forward {
        channel.agent_forward(false).await.map_err(|e| e.to_string())?;
    }
    match hold {
        Some(name) => channel.exec(false, tmux_command(name)?).await.map_err(|e| e.to_string())?,
        None => channel.request_shell(false).await.map_err(|e| e.to_string())?,
    }
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

    Ok(Shell { handle, writer, _forwards: forwards })
}

impl Shell {
    pub async fn write(&self, data: Vec<u8>) -> Result<(), String> {
        self.writer.data_bytes(data).await.map_err(|e| e.to_string())
    }

    pub async fn resize(&self, cols: u32, rows: u32) -> Result<(), String> {
        self.writer.window_change(cols, rows, 0, 0).await.map_err(|e| e.to_string())
    }

    /// Verbindung für zusätzliche Befehle (ohne die Shell-Liste gesperrt zu halten).
    pub fn handle(&self) -> Arc<Handle<Client>> {
        self.handle.clone()
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
    fn tmux_befehl_und_namen() {
        let c = tmux_command("sesam-ab12-0").unwrap();
        assert!(c.contains("new-session -A -s sesam-ab12-0") && c.contains("status off") && c.contains("prefix None") && c.contains("mouse on") && c.contains("set-clipboard on"));
        // Was bei altem tmux scheitern kann, steht hinten.
        assert!(c.find("status off").unwrap() < c.find("set-clipboard").unwrap() && c.find("set-clipboard").unwrap() < c.find("mouse on").unwrap() && c.find("mouse on").unwrap() < c.find("prefix None").unwrap());
        assert!(c.contains("tmux fehlt"));
        assert!(tmux_command("x; rm -rf ~").is_err() && tmux_command("").is_err());
    }

    #[test]
    fn raspberry_erkennen() {
        let p = parse_probe("model=Raspberry Pi 5 Model B Rev 1.0\nid=debian\ntemp=52.1'C\nthrottled=0x50000\n");
        assert_eq!((p.model.as_str(), p.os_id.as_str(), p.temp), ("Raspberry Pi 5 Model B Rev 1.0", "debian", Some(52.1)));
        assert!(!p.undervolt_now && p.undervolt_since_boot);
        let u = parse_probe("model=\nid=\"ubuntu\"\n");
        assert_eq!((u.model.as_str(), u.os_id.as_str(), u.temp), ("", "ubuntu", None));
    }

    #[test]
    fn schluessel_erzeugen_und_eintragen() {
        let (private, public) = generate_ed25519("sesam@test").unwrap();
        check_private_key(&private).unwrap();
        assert!(public.starts_with("ssh-ed25519 ") && public.ends_with(" sesam@test"));
        assert_eq!(public_key_of(&private).unwrap(), public);
        let s = install_key_script(&public).unwrap();
        assert!(s.contains(&public) && s.contains("authorized_keys") && s.contains("grep -qxF"));
        assert!(install_key_script("ssh-ed25519 AAA x'; rm -rf ~; '").is_err());
    }

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

        let legacy = config(Options { legacy: true, keepalive: 30, compression: true, ..Default::default() });
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

/// Server-Journal lesen für die RDP-Fehlersuche: als Benutzer aus «Sesam Test» (Tresor) einen Befehl ausführen.
/// `SESAM_SSH_JOURNAL=192.168.1.20 cargo test --lib probe_journal -- --ignored --nocapture`
/// Befehl über `SESAM_SSH_CMD` (Standard: GNOME-Remote-Desktop-Meldungen der letzten 10 Minuten).
#[cfg(test)]
mod ports_tests {
    #[test]
    fn parse_setup_ports() {
        let p = super::parse_ports("anna 3390\nbenutzer 3391\n\n# kommentar\nsesamtest 3392\nkaputt\nx y\n");
        assert_eq!(p, vec![("anna".into(), 3390), ("benutzer".into(), 3391), ("sesamtest".into(), 3392)]);
    }
}

#[cfg(test)]
mod probe {
    #[tokio::test(flavor = "multi_thread")]
    #[ignore]
    async fn probe_journal() {
        let Ok(host) = std::env::var("SESAM_SSH_JOURNAL") else { return };
        let cmd = std::env::var("SESAM_SSH_CMD").unwrap_or_else(|_| "journalctl --no-pager -o short-precise --since '-10min' -t gnome-remote-de -u gnome-remote-desktop -u gdm | tail -n 200".into());
        let appdata = std::path::PathBuf::from(std::env::var("APPDATA").unwrap()).join("ch.sesam.remote");
        let creds = crate::creds::load(&appdata.join("credentials.json")).unwrap();
        let c = creds.iter().find(|c| c.name == "Sesam Test").expect("«Sesam Test» fehlt");
        let pw = crate::vault::get(&crate::creds::vault_id(&c.id), crate::vault::Kind::Password).unwrap();
        let t = super::Target {
            host,
            port: 22,
            user: c.user.clone(),
            password: pw,
            key: None,
            trusted: std::env::var("SESAM_SSH_TRUST").ok(),
            opts: Default::default(),
            forwards: Vec::new(),
            jump: None,
        };
        let h = match super::connect(&t).await {
            Ok(h) => h,
            Err(e) => return println!("SSH: {e}"),
        };
        let mut ch = h.channel_open_session().await.unwrap();
        ch.exec(true, cmd.as_bytes()).await.unwrap();
        let mut out = Vec::new();
        while let Some(m) = ch.wait().await {
            match m {
                russh::ChannelMsg::Data { data } | russh::ChannelMsg::ExtendedData { data, .. } => out.extend_from_slice(&data),
                russh::ChannelMsg::ExitStatus { .. } | russh::ChannelMsg::Eof => {}
                russh::ChannelMsg::Close => break,
                _ => {}
            }
        }
        println!("{}", String::from_utf8_lossy(&out));
    }
}
