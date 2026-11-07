//! RDP über IronRDP: TLS mit Zertifikat-Pinning, NLA (CredSSP: Kerberos, NTLM als Rückfall), RD-Gateway, Graphics Pipeline (EGFX, H.264 über das System),
//! Display Control (Auflösung folgt dem Fenster) und Server-Redirection (GNOME-Übergabe nach dem Login).
//! Bild und Ereignisse gehen über dieselben `vnc::Event`s wie bei VNC, damit die Anzeige gleich bleibt.

use crate::diag;
use crate::rdp_redir::{self, Redir};
use crate::vnc::Event;
use crate::vnc::decode::Rect;
use crate::vnc::tls::{CERT_CHANGED, CERT_UNKNOWN, fingerprint};
use ironrdp_cliprdr::backend::{ClipboardMessage, ClipboardMessageProxy, CliprdrBackendFactory};
use ironrdp_cliprdr::CliprdrClient;
use ironrdp_connector::{ClientConnector, ConnectionOutcome, ConnectionResult, Credentials, DesktopSize};
use ironrdp_displaycontrol::client::DisplayControlClient;
use ironrdp_displaycontrol::pdu::MonitorLayoutEntry;
use ironrdp_egfx::client::{GraphicsPipelineClient, GraphicsPipelineHandler};
use ironrdp_input::{Database, MouseButton, MousePosition, Operation, Scancode, WheelRotations};
use ironrdp_pdu::gcc;
use ironrdp_pdu::geometry::InclusiveRectangle;
use ironrdp_session::image::DecodedImage;
use ironrdp_session::{ActiveStage, ActiveStageBuilder, ActiveStageOutput};
use ironrdp_connector::Sequence;
use ironrdp_tokio::{FramedWrite, TokioFramed, split_tokio_framed};
use std::sync::{Arc, Mutex};
use tokio::net::TcpStream;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};

/// Höchstens so viele Umleitungen hintereinander (verhindert Schleifen).
const MAX_REDIRECTS: u32 = 8;

#[derive(Clone)]
pub struct Target {
    pub host: String,
    pub port: u16,
    pub user: String,
    pub domain: Option<String>,
    pub password: String,
    /// Erwarteter SHA256 des TLS-Zertifikats (gespeichert oder gerade bestätigt).
    pub expected_cert: Option<String>,
    /// Zertifikat nicht prüfen (Einstellung pro Verbindung, unsicher).
    pub skip_cert_check: bool,
    pub width: u16,
    pub height: u16,
    /// Anzeigeskalierung des Clients in Prozent (100 = keine), wie die Windows-Einstellung «Skalierung».
    pub scale: u32,
    /// Zwischenablage (cliprdr); für jede Verbindung, auch nach Umleitungen, ein neues Backend.
    pub clipboard: Option<ClipFactory>,
    /// Ton des entfernten Rechners hier abspielen.
    pub audio: bool,
    /// Lokaler Ordner, der auf dem Server als Laufwerk erscheint.
    pub share: Option<std::path::PathBuf>,
    /// KDC oder KDC-Proxy für Kerberos (`kdc::kdc_url`), sonst sucht sspi selbst.
    pub kdc: Option<String>,
    /// NTLM verbieten: nur Kerberos (sonst Kerberos zuerst, NTLM als Rückfall).
    pub no_ntlm: bool,
    /// Über ein RD-Gateway verbinden.
    pub gateway: Option<Gateway>,
}

/// RD-Gateway (MS-TSGU, HTTPS/WebSocket). Benutzer als `DOMÄNE\benutzer` oder `benutzer@domain.tld`.
#[derive(Clone)]
pub struct Gateway {
    /// `host[:port]`, Standard 443.
    pub endpoint: String,
    pub user: String,
    pub password: String,
}

pub type ClipFactory = Arc<Mutex<Box<dyn CliprdrBackendFactory + Send>>>;

/// Leitet Meldungen des Zwischenablage-Backends in die Sitzungsschleife.
#[derive(Debug)]
pub struct ClipProxy(pub UnboundedSender<Input>);

impl ClipboardMessageProxy for ClipProxy {
    fn send_clipboard_message(&self, message: ClipboardMessage) {
        let _ = self.0.send(Input::Clipboard(message));
    }
}

/// Eingaben vom Frontend.
pub enum Input {
    /// `buttons`: Bitmaske wie `MouseEvent.buttons` (1 links, 2 rechts, 4 Mitte).
    Pointer { x: u16, y: u16, buttons: u8 },
    Wheel { vertical: bool, units: i16 },
    Scancode { code: u16, down: bool },
    Unicode { ch: char, down: bool },
    /// `scale`: Anzeigeskalierung in Prozent.
    Resize { width: u16, height: u16, scale: u32 },
    /// Mehrere Monitore: Position relativ zum Hauptmonitor, Grösse in Pixeln.
    Monitors(Vec<Monitor>),
    ReleaseAll,
    /// Sauber trennen (Disconnect Provider Ultimatum), damit der Server die Sitzung freigibt.
    Disconnect,
    /// Vom Zwischenablage-Backend.
    Clipboard(ClipboardMessage),
}

#[derive(Clone, Copy, Debug, serde::Deserialize)]
pub struct Monitor {
    pub left: i32,
    pub top: i32,
    pub width: u32,
    pub height: u32,
    pub primary: bool,
}

/// Monitor-Layout für Display Control: Hauptmonitor bei (0,0), Grössen in gültige Werte gerückt.
pub fn monitor_layout(monitors: &[Monitor]) -> Result<ironrdp_displaycontrol::pdu::DisplayControlMonitorLayout, String> {
    let primary = monitors.iter().find(|m| m.primary).or(monitors.first()).ok_or("Keine Monitore")?;
    let mut entries = Vec::new();
    for m in monitors {
        let (w, h) = MonitorLayoutEntry::adjust_display_size(m.width, m.height);
        let is_primary = std::ptr::eq(m, primary);
        let e = if is_primary { MonitorLayoutEntry::new_primary(w, h) } else { MonitorLayoutEntry::new_secondary(w, h) }
            .and_then(|e| if is_primary { Ok(e) } else { e.with_position(m.left - primary.left, m.top - primary.top) })
            .map_err(|e| format!("Monitor-Layout: {e}"))?;
        entries.push(e);
    }
    ironrdp_displaycontrol::pdu::DisplayControlMonitorLayout::new(&entries).map_err(|e| format!("Monitor-Layout: {e}"))
}

/// Der TLS-Zertifikat-Fingerabdruck, den der Server gezeigt hat (für das Pinning nach Bestätigung).
pub type SeenCert = Arc<Mutex<Option<String>>>;

pub struct Session {
    tx: UnboundedSender<Input>,
    task: tokio::task::JoinHandle<()>,
}

impl Session {
    pub fn send(&self, i: Input) {
        let _ = self.tx.send(i);
    }
    /// Trennen wie FreeRDP: MCS «Disconnect Provider Ultimatum» (vom Benutzer gewünscht), dann TCP zu.
    /// Hart abgebrochen wird erst, wenn das nach 2 s nicht geklappt hat.
    pub fn close(&self) {
        let _ = self.tx.send(Input::Disconnect);
        let h = self.task.abort_handle();
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            h.abort();
        });
    }
}

// ---------- Tastatur ----------

/// `KeyboardEvent.code` → Scancode (Set 1), 0xE0xx = erweitert. Physische Taste, das Layout wählt der Server.
pub fn scancode(code: &str) -> Option<u16> {
    const LETTERS: [(char, u16); 26] = [
        ('A', 0x1E), ('B', 0x30), ('C', 0x2E), ('D', 0x20), ('E', 0x12), ('F', 0x21), ('G', 0x22), ('H', 0x23), ('I', 0x17),
        ('J', 0x24), ('K', 0x25), ('L', 0x26), ('M', 0x32), ('N', 0x31), ('O', 0x18), ('P', 0x19), ('Q', 0x10), ('R', 0x13),
        ('S', 0x1F), ('T', 0x14), ('U', 0x16), ('V', 0x2F), ('W', 0x11), ('X', 0x2D), ('Y', 0x15), ('Z', 0x2C),
    ];
    if let Some(l) = code.strip_prefix("Key") {
        let c = l.chars().next()?;
        return LETTERS.iter().find(|(k, _)| *k == c && l.len() == 1).map(|(_, s)| *s);
    }
    if let Some(d) = code.strip_prefix("Digit") {
        let n: u16 = d.parse().ok().filter(|n| *n <= 9)?;
        return Some(if n == 0 { 0x0B } else { 0x01 + n });
    }
    if let Some(f) = code.strip_prefix('F') {
        if let Ok(n) = f.parse::<u16>() {
            return match n {
                1..=10 => Some(0x3A + n),
                11 => Some(0x57),
                12 => Some(0x58),
                _ => None,
            };
        }
    }
    Some(match code {
        "Escape" => 0x01,
        "Minus" => 0x0C,
        "Equal" => 0x0D,
        "Backspace" => 0x0E,
        "Tab" => 0x0F,
        "BracketLeft" => 0x1A,
        "BracketRight" => 0x1B,
        "Enter" => 0x1C,
        "ControlLeft" => 0x1D,
        "Semicolon" => 0x27,
        "Quote" => 0x28,
        "Backquote" => 0x29,
        "ShiftLeft" => 0x2A,
        "Backslash" => 0x2B,
        "Comma" => 0x33,
        "Period" => 0x34,
        "Slash" => 0x35,
        "ShiftRight" => 0x36,
        "NumpadMultiply" => 0x37,
        "AltLeft" => 0x38,
        "Space" => 0x39,
        "CapsLock" => 0x3A,
        "NumLock" => 0x45,
        "ScrollLock" => 0x46,
        "Numpad7" => 0x47,
        "Numpad8" => 0x48,
        "Numpad9" => 0x49,
        "NumpadSubtract" => 0x4A,
        "Numpad4" => 0x4B,
        "Numpad5" => 0x4C,
        "Numpad6" => 0x4D,
        "NumpadAdd" => 0x4E,
        "Numpad1" => 0x4F,
        "Numpad2" => 0x50,
        "Numpad3" => 0x51,
        "Numpad0" => 0x52,
        "NumpadDecimal" => 0x53,
        "IntlBackslash" => 0x56,
        "IntlRo" => 0x73,
        "IntlYen" => 0x7D,
        "NumpadEnter" => 0xE01C,
        "ControlRight" => 0xE01D,
        "NumpadDivide" => 0xE035,
        "PrintScreen" => 0xE037,
        "AltRight" => 0xE038,
        "Home" => 0xE047,
        "ArrowUp" => 0xE048,
        "PageUp" => 0xE049,
        "ArrowLeft" => 0xE04B,
        "ArrowRight" => 0xE04D,
        "End" => 0xE04F,
        "ArrowDown" => 0xE050,
        "PageDown" => 0xE051,
        "Insert" => 0xE052,
        "Delete" => 0xE053,
        "MetaLeft" => 0xE05B,
        "MetaRight" => 0xE05C,
        "ContextMenu" => 0xE05D,
        _ => return None,
    })
}

/// Entscheidet Scancode oder Unicode. Mit AltGr erzeugte Zeichen gehen als Unicode, damit «@», «#» usw.
/// unabhängig vom Tastaturlayout des Servers ankommen; ebenso Tasten ohne bekannten Scancode.
pub fn key_input(code: &str, key: &str, down: bool, altgr: bool) -> Option<Input> {
    let single = {
        let mut it = key.chars();
        it.next().filter(|_| it.next().is_none())
    };
    if code == "AltRight" && key == "AltGraph" {
        return None; // AltGr selbst nicht senden, nur das erzeugte Zeichen
    }
    match (single, scancode(code)) {
        (Some(ch), _) if altgr => Some(Input::Unicode { ch, down }),
        (_, Some(code)) => Some(Input::Scancode { code, down }),
        (Some(ch), None) => Some(Input::Unicode { ch, down }),
        _ => None,
    }
}

// ---------- Verbindung ----------

/// Was eine Server-Redirection für den nächsten Versuch ändert.
#[derive(Default, Clone)]
struct Hop {
    host: Option<String>,
    session_id: u32,
    routing_token: Option<String>,
    cookie: Option<Vec<u8>>,
    /// Umleitung mit verschlüsseltem Passwort: Anmeldung über RDSTLS statt NLA.
    rdstls: Option<crate::rdstls::Creds>,
}

fn utf16_text(b: &[u8]) -> Option<String> {
    if b.len() % 2 != 0 {
        return None;
    }
    let units: Vec<u16> = b.chunks_exact(2).map(|p| u16::from_le_bytes([p[0], p[1]])).take_while(|u| *u != 0).collect();
    String::from_utf16(&units).ok().filter(|s| !s.is_empty() && !s.chars().any(char::is_control))
}

/// Ziel und Anmeldung für den nächsten Versuch anpassen ([MS-RDPBCGR] 3.2.5.13.1).
fn apply_redirect(t: &mut Target, hop: &mut Hop, r: &Redir) -> Result<(), String> {
    // Verschlüsseltes Passwort (GNOME-Übergabe): RDSTLS, Blob unverändert zurück.
    let encrypted = r.password_pk_encrypted && r.password.is_some();
    let routing_token = match r.load_balance_info.as_deref() {
        Some(lb) => {
            let tok = lb.strip_suffix(b"\r\n").unwrap_or(lb);
            let tok = tok.strip_suffix(b"\0").unwrap_or(tok);
            if tok.is_empty() || tok.len() > ironrdp_pdu::nego::MAX_ROUTING_TOKEN_LENGTH || !tok.iter().all(|b| (0x20..=0x7E).contains(b)) {
                return Err("Ungültiges Routing-Token in der Umleitung".into());
            }
            Some(String::from_utf8_lossy(tok).into_owned())
        }
        None => None,
    };
    // Adresse vor Name, wie mstsc; ohne Ziel bleibt der Host (Routing-Token entscheidet).
    let host = r
        .target_net_address
        .iter()
        .chain(r.target_net_addresses.iter())
        .chain(r.target_fqdn.iter())
        .chain(r.target_netbios_name.iter())
        .find(|h| !h.is_empty())
        .cloned();
    if let Some(u) = r.username.as_ref().filter(|u| !u.is_empty()) {
        // GNOME vergibt hier Einmal-Zugangsdaten: wie ein Geheimnis behandeln.
        diag::add_secret(u);
        t.user = u.clone();
    }
    if let Some(d) = &r.domain {
        t.domain = (!d.is_empty()).then(|| d.clone());
    }
    // Sonst nutzen GNOME (und FreeRDP) das Passwort aus der Umleitung auch für NLA.
    if let Some(pw) = r.password.as_deref().filter(|_| !encrypted).and_then(utf16_text) {
        diag::add_secret(&pw);
        t.password = pw;
    }
    // Das Ziel zeigt ein anderes Zertifikat (z. B. das der Benutzersitzung); das aus der Umleitung gilt ohne Rückfrage.
    if let Some(der) = &r.target_cert {
        t.expected_cert = Some(fingerprint(der));
    }
    tracing::info!(
        session_id = r.session_id,
        host = ?host,
        routing_token = routing_token.is_some(),
        cookie = r.password.is_some(),
        rdstls = encrypted,
        target_cert = r.target_cert.is_some(),
        user = r.username.is_some(),
        "Umleitung übernommen"
    );
    let rdstls = encrypted.then(|| crate::rdstls::Creds {
        redirection_guid: r.redirection_guid.clone().unwrap_or_default(),
        user: t.user.clone(),
        domain: t.domain.clone().unwrap_or_default(),
        password_blob: r.password.clone().unwrap_or_default(),
    });
    let cookie = if encrypted { None } else { r.password.clone() };
    *hop = Hop { host, session_id: r.session_id, routing_token, cookie, rdstls };
    Ok(())
}

/// [MS-RDPBCGR] erlaubt für den Gerätefaktor nur 100, 140 und 180 %.
fn device_scale(scale: u32) -> u32 {
    match scale {
        ..=120 => 100,
        121..=160 => 140,
        _ => 180,
    }
}

fn connector_config(t: &Target) -> ironrdp_connector::Config {
    use ironrdp_pdu::rdp::capability_sets::{MajorPlatformType, client_codecs_capabilities};
    ironrdp_connector::Config {
        credentials: Credentials::UsernamePassword { username: t.user.clone(), password: t.password.clone() },
        domain: t.domain.clone(),
        enable_tls: true,
        enable_credssp: true,
        enable_standard_rdp_security: false,
        keyboard_type: gcc::KeyboardType::IBM_ENHANCED,
        keyboard_subtype: 0,
        keyboard_layout: 0,
        keyboard_functional_keys_count: 12,
        connection_type: gcc::ConnectionType::Lan,
        ime_file_name: String::new(),
        dig_product_id: String::new(),
        desktop_size: DesktopSize { width: t.width, height: t.height },
        monitor_layout: None,
        desktop_scale_factor: t.scale,
        desktop_physical_size: None,
        desktop_orientation: None,
        device_scale_factor: Some(device_scale(t.scale)),
        monitor_extended_layout: None,
        relative_mouse_input: false,
        bitmap: Some(ironrdp_connector::BitmapConfig {
            lossy_compression: true,
            color_depth: 32,
            codecs: client_codecs_capabilities(&[]).unwrap_or_default(),
        }),
        client_build: 0,
        client_name: "Sesam Remote".into(),
        client_dir: String::new(),
        platform: if cfg!(windows) { MajorPlatformType::WINDOWS } else { MajorPlatformType::MACINTOSH },
        hardware_id: None,
        license_cache: None,
        enable_server_pointer: true,
        autologon: false,
        enable_audio_playback: t.audio,
        enable_audio_capture: false,
        request_data: None,
        pointer_software_rendering: false,
        multitransport_flags: None,
        support_dyn_vc_gfx_protocol: true,
        compression_type: None,
        performance_flags: Default::default(),
        timezone_info: Default::default(),
        alternate_shell: String::new(),
        work_dir: String::new(),
        remote_application_mode: false,
        rail_support_level: ironrdp_pdu::rdp::capability_sets::RailSupportLevel::empty(),
    }
}

struct NoGfxCallbacks;
impl GraphicsPipelineHandler for NoGfxCallbacks {}

/// TCP direkt oder Tunnel durch das RD-Gateway.
enum Stream {
    Tcp(TcpStream),
    Gateway(Box<ironrdp_mstsgu::GwClient>),
}

macro_rules! by_stream {
    ($self:ident, $s:ident => $e:expr) => {
        match $self.get_mut() {
            Stream::Tcp($s) => { let $s = std::pin::Pin::new($s); $e }
            Stream::Gateway($s) => { let $s = std::pin::Pin::new($s.as_mut()); $e }
        }
    };
}

impl tokio::io::AsyncRead for Stream {
    fn poll_read(self: std::pin::Pin<&mut Self>, cx: &mut std::task::Context<'_>, buf: &mut tokio::io::ReadBuf<'_>) -> std::task::Poll<std::io::Result<()>> {
        by_stream!(self, s => s.poll_read(cx, buf))
    }
}

impl tokio::io::AsyncWrite for Stream {
    fn poll_write(self: std::pin::Pin<&mut Self>, cx: &mut std::task::Context<'_>, buf: &[u8]) -> std::task::Poll<std::io::Result<usize>> {
        by_stream!(self, s => s.poll_write(cx, buf))
    }
    fn poll_flush(self: std::pin::Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> std::task::Poll<std::io::Result<()>> {
        by_stream!(self, s => s.poll_flush(cx))
    }
    fn poll_shutdown(self: std::pin::Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> std::task::Poll<std::io::Result<()>> {
        by_stream!(self, s => s.poll_shutdown(cx))
    }
}
type Framed = TokioFramed<ironrdp_tls::TlsStream<Stream>>;

/// Transport zum RDP-Server: direkt per TCP oder durch das RD-Gateway.
async fn transport(t: &Target, host: &str) -> Result<(Stream, std::net::SocketAddr), String> {
    if let Some(gw) = &t.gateway {
        tracing::info!(gateway = %gw.endpoint, %host, port = t.port, "RDP: RD-Gateway");
        let target = ironrdp_mstsgu::GwConnectTarget {
            gw_endpoint: gw.endpoint.clone(),
            gw_user: gw.user.clone(),
            gw_pass: gw.password.clone(),
            smart_card: None,
            server: host.to_string(),
        };
        // Gateway-Zertifikat strikt gegen die Stammzertifikate des Systems (wie mstsc), kein Pinning.
        let (gw, addr) = tokio::time::timeout(
            std::time::Duration::from_secs(30),
            ironrdp_mstsgu::GwClient::connect_with_port(&target, "Sesam Remote", t.port),
        )
        .await
        .map_err(|_| "RD-Gateway: Zeitüberschreitung".to_string())?
        .map_err(|e| phase("RD-Gateway", e))?;
        return Ok((Stream::Gateway(Box::new(gw)), addr));
    }
    tracing::info!(%host, port = t.port, "RDP: TCP");
    let tcp = tokio::time::timeout(std::time::Duration::from_secs(15), TcpStream::connect((host, t.port)))
        .await
        .map_err(|_| "TCP: Zeitüberschreitung beim Verbinden".to_string())?
        .map_err(|e| format!("TCP: Verbindung fehlgeschlagen: {e}"))?;
    let _ = tcp.set_nodelay(true);
    let addr = tcp.local_addr().map_err(|e| e.to_string())?;
    Ok((Stream::Tcp(tcp), addr))
}

/// CredSSP wie in ironrdp-async, aber mit eigenem KDC-Transport, Rückfall auf NTLM bei KDC-Fehlern,
/// NTLM-Sperre pro Verbindung und ohne Rückfall auf CredSSP-Versionen vor 5 (CVE-2018-0886).
async fn credssp(t: &Target, server: &str, connector: &mut ClientConnector, framed: &mut Framed, key: Vec<u8>) -> Result<(), String> {
    use ironrdp_connector::credssp::{CredsspSequence, KerberosConfig};
    use ironrdp_connector::sspi::credssp::ClientState;
    use ironrdp_connector::sspi::generator::GeneratorState;
    use ironrdp_connector::sspi::{Error as SspiError, ErrorKind};
    let ironrdp_connector::ClientConnectorState::Credssp { selected_protocol, .. } = connector.state else {
        return Err("CredSSP: unerwarteter Zustand".into());
    };
    let kerberos = crate::kdc::wants_kerberos(&t.user, t.domain.as_deref(), t.kdc.is_some(), t.no_ntlm)
        .then(|| KerberosConfig::new(t.kdc.clone(), crate::kdc::computer_name()))
        .transpose()
        .map_err(login_err)?;
    tracing::info!(kerberos = kerberos.is_some(), kdc = ?t.kdc, no_ntlm = t.no_ntlm, "RDP: CredSSP");
    let (mut seq, mut ts) = CredsspSequence::init(
        connector.config.credentials.clone(),
        connector.config.domain.as_deref(),
        selected_protocol,
        ironrdp_connector::ServerName::new(server),
        key,
        kerberos,
    )
    .map_err(login_err)?;
    let mut buf = ironrdp_pdu::WriteBuf::new();
    loop {
        let state = {
            let mut generator = seq.process_ts_request(ts);
            let mut step = generator.start();
            loop {
                match step {
                    GeneratorState::Suspended(req) => {
                        // KDC nicht erreichbar: sspi entscheidet über den Rückfall auf NTLM.
                        let reply = crate::kdc::send(&req).await.map_err(|e| SspiError::new(ErrorKind::NoAuthenticatingAuthority, e));
                        step = generator.resume(reply);
                    }
                    GeneratorState::Completed(r) => break r,
                }
            }
        }
        .map_err(login_err)?;
        let (ClientState::ReplyNeeded(req) | ClientState::FinalMessage(req)) = &state;
        if t.no_ntlm && req.nego_tokens.as_deref().is_some_and(crate::kdc::carries_ntlm) {
            return Err("Anmeldung: Kerberos war nicht möglich und NTLM ist für diese Verbindung nicht erlaubt. \
                        Benutzer als benutzer@domain.tld eingeben und den Servernamen (nicht die IP) verwenden; \
                        ist kein KDC per DNS erreichbar, unter «Erweitert» einen KDC oder KDC-Proxy eintragen."
                .into());
        }
        buf.clear();
        let written = seq.handle_process_result(state, &mut buf).map_err(login_err)?;
        if let Some(n) = written.size() {
            framed.write_all(&buf[..n]).await.map_err(|e| format!("CredSSP: {e}"))?;
        }
        let Some(hint) = seq.next_pdu_hint() else { break };
        let pdu = framed.read_by_hint(hint).await.map_err(login_err)?;
        match seq.decode_server_message(&pdu).map_err(login_err)? {
            Some(next) if next.version < 5 => {
                return Err(format!(
                    "Anmeldung: Der Server spricht nur CredSSP {} (ungepatcht, CVE-2018-0886). Bitte den Server aktualisieren.",
                    next.version
                ));
            }
            Some(next) => ts = next,
            None => break,
        }
    }
    connector.mark_credssp_as_done();
    Ok(())
}

/// Ein Verbindungsversuch bis zum Ende des Verbindungsaufbaus.
async fn connect_once(t: &Target, hop: &Hop, seen: &SeenCert) -> Result<(ConnectionOutcome, Framed), String> {
    let host = hop.host.clone().unwrap_or_else(|| t.host.clone());
    let redirected = hop.session_id != 0 || hop.routing_token.is_some();
    let user_log = if redirected { "***" } else { t.user.as_str() };
    tracing::info!(%host, port = t.port, user = %user_log, redirected, "RDP: Verbindung");
    // ring als Standard: mit aws-lc-rs (über sspi/ironrdp-mstsgu) im Baum wählt rustls sonst nicht selbst.
    let _ = rustls::crypto::ring::default_provider().install_default();
    let (tcp, client_addr) = transport(t, &host).await?;

    let mut drdynvc = ironrdp_dvc::DrdynvcClient::new()
        .with_dynamic_channel(DisplayControlClient::new(|_| Ok(Vec::new())))
        .with_dynamic_channel(GraphicsPipelineClient::new(Box::new(NoGfxCallbacks), crate::h264::decoder()));
    if t.audio {
        // Neuere Server nehmen den dynamischen Kanal, ältere den statischen; je ein eigener Ausgang.
        drdynvc = drdynvc.with_dynamic_channel(ironrdp_rdpsnd::client::RdpsndDvc::new(Box::new(ironrdp_rdpsnd_native::cpal::RdpsndBackend::new())));
    }
    let mut connector = ClientConnector::new(connector_config(t), client_addr)
        .with_static_channel(drdynvc)
        .with_cluster_data(gcc::ClientClusterData {
            flags: gcc::RedirectionFlags::REDIRECTION_SUPPORTED | gcc::RedirectionFlags::REDIRECTED_SESSION_FIELD_VALID,
            redirection_version: gcc::RedirectionVersion::V6,
            redirected_session_id: hop.session_id,
        });
    if let Some(tok) = &hop.routing_token {
        connector = connector.with_load_balance_info(tok.clone());
    }
    if let Some(cookie) = &hop.cookie {
        connector = connector.with_redirection_password_cookie(cookie.clone());
    }
    if t.audio {
        connector = connector.with_static_channel(ironrdp_rdpsnd::client::Rdpsnd::new(Box::new(ironrdp_rdpsnd_native::cpal::RdpsndBackend::new())));
    }
    if let Some(f) = &t.clipboard {
        connector = connector.with_static_channel(CliprdrClient::new(f.lock().unwrap().build_cliprdr_backend()));
    }
    if let Some(dir) = &t.share {
        connector = connector.with_static_channel(drive_channel(dir)?);
    }

    let mut framed = TokioFramed::new(tcp);
    tracing::info!(rdstls = hop.rdstls.is_some(), "RDP: Aushandlung (X.224)");
    let should_upgrade = if hop.rdstls.is_some() {
        // IronRDP fordert RDSTLS nicht an: Aushandlung selbst, Connector dann direkt beim TLS-Upgrade weiter.
        let confirm = rdstls_negotiate(&mut framed, hop).await?;
        // Die Antwort durch den Connector schicken: er übernimmt so die Antwort-Flags (EXTENDED_CLIENT_DATA_SUPPORTED)
        // und fordert den MCS-Nachrichtenkanal an. Ohne ihn verwirft GNOME die Auto-Detect-Antworten («message channel id == 0»).
        connector.state = ironrdp_connector::ClientConnectorState::ConnectionInitiationWaitConfirm {
            requested_protocol: ironrdp_pdu::nego::SecurityProtocol::RDSTLS | ironrdp_pdu::nego::SecurityProtocol::SSL,
        };
        connector.step(&confirm, None, &mut ironrdp_pdu::WriteBuf::new()).map_err(|e| phase("Aushandlung", e))?;
        ironrdp_tokio::skip_connect_begin(&mut connector)
    } else {
        ironrdp_tokio::connect_begin(&mut framed, &mut connector).await.map_err(|e| phase("Aushandlung", e))?
    };
    let (tcp, leftover) = framed.into_inner();

    // Zertifikate mit gültiger Kette akzeptiert rustls selbst; sonst entscheidet das Pinning.
    let expected = t.expected_cert.clone();
    let seen2 = seen.clone();
    let skip = t.skip_cert_check;
    if skip {
        tracing::warn!("RDP: Zertifikatsprüfung ist für diese Verbindung ausgeschaltet");
    }
    let callback: ironrdp_tls::CertificateValidationCallback = Arc::new(move |der: &[u8], _: &str, _: &str| {
        let fp = fingerprint(der);
        let ok = skip || expected.as_deref() == Some(fp.as_str());
        *seen2.lock().unwrap() = Some(fp);
        ok
    });
    tracing::info!("RDP: TLS");
    let (tls, cert) = ironrdp_tls::upgrade_with_certificate_validation_callback(tcp, &t.host, callback).await.map_err(|e| {
        let seen = seen.lock().unwrap().clone();
        tracing::warn!(error = %e, seen = ?seen, expected = ?t.expected_cert, "RDP: TLS gescheitert");
        match (seen, &t.expected_cert) {
            (Some(fp), None) => format!("{CERT_UNKNOWN}{fp}"),
            (Some(fp), Some(want)) if &fp != want => format!("{CERT_CHANGED}{fp}"),
            _ => format!("TLS: Verbindung fehlgeschlagen: {e}"),
        }
    })?;
    let mut tls = tls;
    if let Some(creds) = &hop.rdstls {
        tracing::info!("RDP: RDSTLS-Anmeldung");
        crate::rdstls::handshake(&mut tls, creds).await?;
    }
    let upgraded = ironrdp_tokio::mark_as_upgraded(should_upgrade, &mut connector);
    let key = ironrdp_tls::extract_tls_server_public_key(&cert).ok_or("Öffentlicher Schlüssel des Servers fehlt")?.to_vec();
    let mut framed = TokioFramed::new_with_leftover(tls, leftover);
    if hop.rdstls.is_some() {
        tracing::info!("RDP: Verbindungsaufbau nach RDSTLS (ohne CredSSP)");
        let outcome = finalize_tolerant(connector, &mut framed).await?;
        tracing::info!(redirected = matches!(outcome, ConnectionOutcome::Redirected(_)), "RDP: verbunden");
        return Ok((outcome, framed));
    }
    tracing::info!("RDP: Anmeldung (NLA/CredSSP) und Verbindungsaufbau");
    let check = CHECK_ONLY.try_with(|v| *v).unwrap_or(false);
    if connector.should_perform_credssp() {
        credssp(t, &host, &mut connector, &mut framed, key.clone()).await?;
    } else if check {
        return Err("Dieser Server prüft die Zugangsdaten erst am Bildschirm, nicht vorab.".into());
    }
    if check {
        // Nur prüfen: nach der Anmeldung sauber trennen, kein Desktop, kein Anmeldebildschirm.
        let _ = framed.write_all(&disconnect_frame()).await;
        return Err(CHECK_PASSED.into());
    }
    let outcome = ironrdp_tokio::connect_finalize(
        upgraded,
        connector,
        &mut framed,
        &mut crate::kdc::Kdc,
        ironrdp_connector::ServerName::new(host.clone()),
        key,
        None,
    )
    .await
    .map_err(login_err)?;
    tracing::info!(redirected = matches!(outcome, ConnectionOutcome::Redirected(_)), "RDP: verbunden");
    Ok((outcome, framed))
}

/// Ist das ein Lizenz-Paket (Basic Security Header mit SEC_LICENSE_PKT)? `None`, wenn kein MCS-Datenpaket.
fn license_flag(pdu: &[u8]) -> Option<bool> {
    let ctx = ironrdp_pdu::mcs::decode_send_data_indication(pdu).ok()?;
    let d = ctx.user_data;
    (d.len() >= 2).then(|| u16::from_le_bytes([d[0], d[1]]) & 0x0080 != 0)
}

/// Auto-Detect beim Verbindungsaufbau ([MS-RDPBCGR] 2.2.14), wie es gnome-remote-desktop nach RDSTLS
/// ohne ausgehandelten Nachrichtenkanal schickt (IronRDP wartet darauf nur mit Nachrichtenkanal).
/// `None` = kein Auto-Detect-Paket; sonst die nötige Antwort (RTT, Bandbreiten-Ergebnis) oder nichts.
/// Ohne das Ergebnis der Bandbreitenmessung wartet ein FreeRDP-Server (GNOME) endlos.
fn connect_time_autodetect(pdu: &[u8], user_channel_id: u16, bw: &mut Option<(std::time::Instant, u32)>) -> Option<Option<Vec<u8>>> {
    use ironrdp_pdu::rdp::autodetect::{AutoDetectReqPdu, AutoDetectRequest as Q, AutoDetectResponse as A, AutoDetectRspPdu, BW_RESULTS_CONNECT_TIME, BW_START_CONNECT_TIME, BW_STOP_CONNECT_TIME};
    let ctx = ironrdp_pdu::mcs::decode_send_data_indication(pdu).ok()?;
    let d = ctx.user_data;
    if d.len() < 2 || u16::from_le_bytes([d[0], d[1]]) & 0x1000 == 0 {
        return None; // kein SEC_AUTODETECT_REQ
    }
    let req = ironrdp_pdu::decode::<AutoDetectReqPdu>(d).ok()?.request;
    tracing::debug!(?req, channel = ctx.channel_id, "Auto-Detect beim Verbindungsaufbau");
    // Gezählt wie in IronRDP/FreeRDP: Nutzlast plus 8 Byte Kopf.
    let counted = |p: &[u8]| u32::try_from(p.len()).unwrap_or(u32::MAX).saturating_add(8);
    let rsp = match req {
        Q::RttRequest { sequence_number, .. } => Some(A::RttResponse { sequence_number }),
        Q::BandwidthMeasureStart { request_type, .. } if request_type == BW_START_CONNECT_TIME => {
            *bw = Some((std::time::Instant::now(), 0));
            None
        }
        Q::BandwidthMeasurePayload { payload, .. } => {
            if let Some((_, n)) = bw.as_mut() {
                *n = n.saturating_add(counted(&payload));
            }
            None
        }
        Q::BandwidthMeasureStop { sequence_number, request_type, payload } if request_type == BW_STOP_CONNECT_TIME => {
            let last = payload.as_deref().map_or(0, counted);
            let (time_delta_ms, byte_count) = match bw.take() {
                Some((t, n)) => (u32::try_from(t.elapsed().as_millis()).unwrap_or(u32::MAX).max(1), n.saturating_add(last)),
                None => (1, last),
            };
            Some(A::BandwidthMeasureResults { sequence_number, response_type: BW_RESULTS_CONNECT_TIME, time_delta_ms, byte_count })
        }
        _ => None,
    };
    Some(rsp.and_then(|r| {
        // Antwort auf dem Kanal der Anfrage (bei GNOME ohne Nachrichtenkanal: 0).
        let mut w = ironrdp_pdu::WriteBuf::new();
        ironrdp_pdu::mcs::encode_send_data_request(user_channel_id, ctx.channel_id, &AutoDetectRspPdu::new(r), &mut w).ok()?;
        Some(w.filled().to_vec())
    }))
}

/// Wie `connect_finalize` (ohne CredSSP), aber nachsichtig: Überspringt der Server die Lizenzierung
/// (gnome-remote-desktop nach RDSTLS), geht es direkt mit dem Capabilities-Austausch weiter.
async fn finalize_tolerant(mut connector: ClientConnector, framed: &mut Framed) -> Result<ConnectionOutcome, String> {
    use ironrdp_connector::{ClientConnectorState, MultitransportResult, State as _};
    let mut buf = ironrdp_pdu::WriteBuf::new();
    let mut bw = None;
    loop {
        if connector.should_perform_multitransport() {
            ironrdp_tokio::complete_multitransport(
                &mut connector,
                framed,
                MultitransportResult::Failure(ironrdp_pdu::rdp::multitransport::MultitransportResponsePdu::E_ABORT),
            )
            .await
            .map_err(|e| phase("Verbindungsaufbau", e))?;
            continue;
        }
        buf.clear();
        let written = match connector.next_pdu_hint() {
            Some(hint) => {
                let pdu = framed.read_by_hint(hint).await.map_err(|e| format!("Verbindungsaufbau: Verbindung beendet: {e}"))?;
                if let ClientConnectorState::LicensingExchange { user_channel_id, .. } | ClientConnectorState::MultitransportBootstrapping { user_channel_id, .. } = &connector.state {
                    if let Some(reply) = connect_time_autodetect(&pdu, *user_channel_id, &mut bw) {
                        if let Some(r) = reply {
                            framed.write_all(&r).await.map_err(|e| format!("Verbindungsaufbau: {e}"))?;
                        }
                        continue;
                    }
                }
                if let ClientConnectorState::LicensingExchange { io_channel_id, user_channel_id, .. } = &connector.state {
                    if license_flag(&pdu) == Some(false) {
                        let head: String = pdu.iter().take(32).map(|b| format!("{b:02x}")).collect();
                        tracing::info!(head = %head, "Server überspringt die Lizenzierung");
                        connector.state = ClientConnectorState::MultitransportBootstrapping {
                            io_channel_id: *io_channel_id,
                            user_channel_id: *user_channel_id,
                            message_channel_id: connector.message_channel_id,
                            requests_seen: 0,
                        };
                    }
                }
                let state = connector.state.name();
                connector.step(&pdu, framed.last_read_at(), &mut buf).map_err(|e| {
                    let head: String = pdu.iter().take(48).map(|b| format!("{b:02x}")).collect();
                    tracing::warn!(state, len = pdu.len(), head = %head, "Verbindungsaufbau: Paket nicht lesbar");
                    phase("Verbindungsaufbau", e)
                })?
            }
            None => connector.step_no_input(&mut buf).map_err(|e| phase("Verbindungsaufbau", e))?,
        };
        if written.size().is_some() {
            framed.write_all(buf.filled()).await.map_err(|e| format!("Verbindungsaufbau: {e}"))?;
        }
        if let Some(outcome) = connector.state.take_outcome() {
            return Ok(outcome);
        }
    }
}

tokio::task_local! {
    static CHECK_ONLY: bool;
}
const CHECK_PASSED: &str = "__sesam_check_passed__";

/// «Zugangsdaten prüfen»: verbinden bis die Anmeldung (NLA/CredSSP) durch ist, dann sauber trennen.
pub async fn check_login(t: Target, seen: SeenCert) -> Result<(), String> {
    diag::add_secret(&t.password);
    match CHECK_ONLY.scope(true, connect_once(&t, &Hop::default(), &seen)).await {
        Err(e) if e == CHECK_PASSED => Ok(()),
        Err(e) => Err(e),
        Ok(_) => Ok(()),
    }
}

/// MCS Disconnect Provider Ultimatum mit Grund «vom Benutzer gewünscht» ([MS-RDPBCGR] 1.3.1.4.1).
fn disconnect_frame() -> Vec<u8> {
    use ironrdp_pdu::mcs::{DisconnectProviderUltimatum, DisconnectReason};
    ironrdp_pdu::encode_vec(&ironrdp_pdu::x224::X224(DisconnectProviderUltimatum { reason: DisconnectReason::UserRequested })).unwrap_or_default()
}

/// X.224-Aushandlung mit PROTOCOL_RDSTLS und Routing-Token; der Server muss RDSTLS wählen.
async fn rdstls_negotiate(framed: &mut TokioFramed<Stream>, hop: &Hop) -> Result<Vec<u8>, String> {
    use ironrdp_pdu::nego::{ConnectionConfirm, ConnectionRequest, ConnectionRequestWithOpaqueRoutingToken, OpaqueRoutingToken, RequestFlags, SecurityProtocol};
    use ironrdp_pdu::x224::X224;
    let request = ConnectionRequest {
        nego_data: None,
        flags: RequestFlags::empty(),
        protocol: SecurityProtocol::RDSTLS | SecurityProtocol::SSL,
        correlation_info: None,
    };
    let bytes = match &hop.routing_token {
        Some(tok) => ironrdp_pdu::encode_vec(&X224(ConnectionRequestWithOpaqueRoutingToken { request, routing_token: OpaqueRoutingToken(tok.clone()) })),
        None => ironrdp_pdu::encode_vec(&X224(request)),
    }
    .map_err(|e| format!("Aushandlung: {e}"))?;
    framed.write_all(&bytes).await.map_err(|e| format!("Aushandlung: {e}"))?;
    let (_, frame) = framed.read_pdu().await.map_err(|e| format!("Aushandlung: {e}"))?;
    match ironrdp_pdu::decode::<X224<ConnectionConfirm>>(&frame).map_err(|e| format!("Aushandlung: {e}"))?.0 {
        ConnectionConfirm::Response { protocol, flags } if protocol.contains(SecurityProtocol::RDSTLS) => {
            tracing::info!(?protocol, ?flags, "RDSTLS gewählt");
            Ok(frame.to_vec())
        }
        ConnectionConfirm::Response { protocol, .. } => Err(format!("Aushandlung: Server wählt {protocol:?} statt RDSTLS für die Übergabe")),
        ConnectionConfirm::Failure { code } => Err(format!("Aushandlung: Server lehnt ab ({code:?})")),
    }
}

/// Name des Laufwerks auf dem Server: Ordnername, nur einfache Zeichen.
pub fn share_name(dir: &std::path::Path) -> String {
    let n: String = dir
        .file_name()
        .map(|n| n.to_string_lossy().chars().filter(|c| c.is_alphanumeric() || " -_".contains(*c)).take(32).collect())
        .unwrap_or_default();
    if n.trim().is_empty() { "Sesam".into() } else { n }
}

/// Neues rdpdr-Backend pro Verbindung (Handles gehören zur Sitzung).
fn drive_channel(dir: &std::path::Path) -> Result<ironrdp_rdpdr::Rdpdr, String> {
    use ironrdp_rdpdr::backend::RdpdrBackendFactory;
    if !dir.is_absolute() || !dir.is_dir() {
        return Err(format!("Freigegebener Ordner nicht gefunden: {}", dir.display()));
    }
    let name = share_name(dir);
    #[cfg(windows)]
    let factory = {
        let drive = ironrdp_rdpdr_native::RedirectedDrive::new(ironrdp_rdpdr_native::FIRST_NAMED_DRIVE_DEVICE_ID, name, dir, false)
            .map_err(|e| format!("Freigabe: {e}"))?;
        ironrdp_rdpdr_native::WindowsRdpdrBackendFactory::new(drive)
    };
    #[cfg(not(windows))]
    let factory = ironrdp_rdpdr_native::backend::NixRdpdrBackendFactory::new(dir.to_string_lossy().into_owned()).with_drive(1, name);
    let product = factory.build_rdpdr_backend().map_err(|e| format!("Freigabe: {e}"))?;
    let (backend, drives) = product.into_parts();
    let drives = drives.iter().map(|d| (d.device_id(), d.name().to_string())).collect();
    Ok(ironrdp_rdpdr::Rdpdr::new(backend, "Sesam Remote".into()).with_drives(Some(drives)))
}

/// Ein gefundenes Paket ist nur dann eine echte Umleitung, wenn es ein Ziel oder Anmeldedaten trägt.
fn useful_redirect(r: &Redir) -> bool {
    r.load_balance_info.is_some() || r.password.is_some() || r.target_net_address.is_some() || r.target_fqdn.is_some()
}

/// Fehler mit allen Ursachen, ohne Geheimnisse; landet auch im Log.
fn err<E: std::error::Error + 'static>(e: E) -> String {
    let s = diag::chain(&e);
    tracing::warn!(error = %s, "RDP-Fehler");
    s
}

fn phase<E: std::error::Error + 'static>(name: &str, e: E) -> String {
    format!("{name}: {}", err(e))
}

/// Fehler bei Anmeldung und Verbindungsaufbau: Ablehnung klar von Protokollfehlern trennen.
fn login_err<E: std::error::Error + 'static>(e: E) -> String {
    let s = err(e);
    let lower = s.to_lowercase();
    if ["logon_failure", "logonfailure", "logon failure", "sec_e_logon_denied", "access_denied", "access denied", "wrong password"].iter().any(|k| lower.contains(k)) {
        "Anmeldung abgelehnt: Benutzer oder Passwort stimmen nicht.".into()
    } else if ["read frame", "eof", "connection reset", "10054", "broken pipe"].iter().any(|k| lower.contains(k)) {
        format!(
            "Anmeldung: Der Server hat die Verbindung beendet. Meist sind die RDP-Zugangsdaten falsch oder auf dem Server nicht gesetzt (grdctl … set-credentials). Details: {s}"
        )
    } else {
        format!("Anmeldung/Verbindungsaufbau: {s}")
    }
}

pub fn channel() -> (UnboundedSender<Input>, UnboundedReceiver<Input>) {
    unbounded_channel()
}

/// Verbinden (inkl. Umleitungen während des Aufbaus) und Sitzung starten.
/// `tx`/`rx` aus [`channel`]; `tx` darf vorher schon an das Zwischenablage-Backend gegangen sein.
pub async fn open(
    mut t: Target,
    seen: SeenCert,
    (tx, rx): (UnboundedSender<Input>, UnboundedReceiver<Input>),
    emit: impl Fn(Event) -> bool + Send + Sync + 'static,
) -> Result<Session, String> {
    diag::add_secret(&t.password);
    let mut hop = Hop::default();
    let mut redirects = 0;
    let (result, framed) = loop {
        let (outcome, framed) = connect_once(&t, &hop, &seen).await?;
        match outcome {
            ConnectionOutcome::Connected(r) => break (r, framed),
            ConnectionOutcome::Redirected(r) => {
                redirects += 1;
                if redirects > MAX_REDIRECTS {
                    return Err("Zu viele Umleitungen".into());
                }
                apply_redirect(&mut t, &mut hop, &Redir::from(&*r))?;
            }
        }
    };
    let task = tokio::spawn(run(t, hop, seen, result, framed, rx, emit, redirects));
    Ok(Session { tx, task })
}

enum Flow {
    Closed(String),
    Redirect(Redir),
}

#[allow(clippy::too_many_arguments)]
async fn run(
    mut t: Target,
    mut hop: Hop,
    seen: SeenCert,
    mut result: ConnectionResult,
    mut framed: Framed,
    mut rx: UnboundedReceiver<Input>,
    emit: impl Fn(Event) -> bool + Send + Sync + 'static,
    mut redirects: u32,
) {
    loop {
        let flow = active(result, framed, &mut rx, &emit).await;
        let why = match flow {
            Ok(Flow::Closed(why)) => why,
            Err(e) => e,
            // Umleitung im laufenden Betrieb, z. B. GNOME nach dem Login am Anmeldebildschirm.
            Ok(Flow::Redirect(r)) => {
                redirects += 1;
                let next = if redirects > MAX_REDIRECTS {
                    Err("Zu viele Umleitungen".into())
                } else {
                    emit(Event::Info("Sitzung wird übergeben …".into()));
                    match apply_redirect(&mut t, &mut hop, &r) {
                        Ok(()) => reconnect(&mut t, &mut hop, &seen, &mut redirects).await,
                        Err(e) => Err(e),
                    }
                };
                match next {
                    Ok((r, f)) => {
                        emit(Event::Info(String::new()));
                        result = r;
                        framed = f;
                        continue;
                    }
                    Err(e) => e,
                }
            }
        };
        emit(Event::Closed(why));
        return;
    }
}

async fn reconnect(t: &mut Target, hop: &mut Hop, seen: &SeenCert, redirects: &mut u32) -> Result<(ConnectionResult, Framed), String> {
    loop {
        let (outcome, framed) = connect_once(t, hop, seen).await?;
        match outcome {
            ConnectionOutcome::Connected(r) => return Ok((r, framed)),
            ConnectionOutcome::Redirected(r) => {
                *redirects += 1;
                if *redirects > MAX_REDIRECTS {
                    return Err("Zu viele Umleitungen".into());
                }
                apply_redirect(t, hop, &Redir::from(&*r))?;
            }
        }
    }
}

fn rect_of(r: &InclusiveRectangle, img: &DecodedImage) -> Option<Rect> {
    let (w, h) = (img.width(), img.height());
    let right = r.right.min(w.saturating_sub(1));
    let bottom = r.bottom.min(h.saturating_sub(1));
    (r.left <= right && r.top <= bottom).then(|| Rect { x: r.left, y: r.top, w: right - r.left + 1, h: bottom - r.top + 1 })
}

fn crop(img: &DecodedImage, r: Rect) -> Vec<u8> {
    let (stride, data) = (img.stride(), img.data());
    let mut out = Vec::with_capacity(r.w as usize * r.h as usize * 4);
    for row in r.y as usize..(r.y + r.h) as usize {
        let s = row * stride + r.x as usize * 4;
        out.extend_from_slice(&data[s..s + r.w as usize * 4]);
    }
    // Alpha ist bei RDP bedeutungslos; immer deckend zeichnen.
    out.chunks_exact_mut(4).for_each(|p| p[3] = 255);
    out
}

async fn active(
    result: ConnectionResult,
    framed: Framed,
    rx: &mut UnboundedReceiver<Input>,
    emit: &(impl Fn(Event) -> bool + Send + Sync),
) -> Result<Flow, String> {
    let (mut reader, mut writer) = split_tokio_framed(framed);
    let size = result.desktop_size;
    let mut image = DecodedImage::new(ironrdp_graphics::image_processing::PixelFormat::RgbA32, size.width, size.height);
    let mut shown = (image.width(), image.height());
    let activation = result.activation_factory;
    let mut stage: ActiveStage = ActiveStageBuilder {
        static_channels: result.static_channels,
        user_channel_id: result.user_channel_id,
        io_channel_id: result.io_channel_id,
        message_channel_id: result.message_channel_id,
        share_id: result.share_id,
        compression_type: result.compression_type,
        enable_server_pointer: result.enable_server_pointer,
        pointer_software_rendering: result.pointer_software_rendering,
    }
    .build();
    let mut input = Database::new();
    if !emit(Event::Resize { width: size.width, height: size.height }) {
        return Ok(Flow::Closed("Fenster geschlossen".into()));
    }
    // Zeitüberschreitungen der Zwischenablage (Sperren) regelmässig abarbeiten.
    let mut clip_tick = tokio::time::interval(std::time::Duration::from_secs(5));
    // Grössenwunsch (Breite, Höhe, Skalierung), bis der Anzeigekanal bereit ist.
    let mut pending_resize: Option<(u32, u32, u32)> = None;
    let mut resize_hint = false;
    let started = std::time::Instant::now();

    loop {
        let outputs = tokio::select! {
            _ = clip_tick.tick() => {
                let Some(c) = stage.get_svc_processor_mut::<CliprdrClient>() else { continue };
                let msgs = c.drive_timeouts().map_err(err)?;
                let frame = stage.process_svc_processor_messages(msgs).map_err(err)?;
                if !frame.is_empty() {
                    writer.write_all(&frame).await.map_err(|e| e.to_string())?;
                }
                continue;
            }
            frame = reader.read_pdu() => {
                let (action, payload) = frame.map_err(|e| format!("Sitzung: Verbindung beendet: {e}"))?;
                match stage.process(&mut image, action, &payload, reader.last_read_at()) {
                    Ok(out) => out,
                    Err(e) => {
                        // IronRDP scheitert an GNOMEs Umleitung nach dem Login: selbst nachsehen.
                        if let Some(r) = rdp_redir::find(&payload).filter(useful_redirect) {
                            tracing::info!(error = %diag::chain(&e), "Umleitung über eigenen Decoder erkannt");
                            return Ok(Flow::Redirect(r));
                        }
                        let head: String = payload.iter().take(24).map(|b| format!("{b:02x}")).collect();
                        tracing::warn!(len = payload.len(), head = %head, "Rahmen nicht lesbar");
                        return Err(format!("Sitzung: {}", err(e)));
                    }
                }
            }
            msg = rx.recv() => {
                let Some(msg) = msg.filter(|m| !matches!(m, Input::Disconnect)) else {
                    let _ = writer.write_all(&disconnect_frame()).await;
                    return Ok(Flow::Closed("Getrennt".into()));
                };
                let ops: Vec<Operation> = match msg {
                    Input::Pointer { x, y, buttons } => {
                        let mut ops = vec![Operation::MouseMove(MousePosition { x, y })];
                        for (bit, b) in [(1u8, MouseButton::Left), (2, MouseButton::Right), (4, MouseButton::Middle)] {
                            let want = buttons & bit != 0;
                            if want != input.is_mouse_button_pressed(b) {
                                ops.push(if want { Operation::MouseButtonPressed(b) } else { Operation::MouseButtonReleased(b) });
                            }
                        }
                        ops
                    }
                    Input::Wheel { vertical, units } => vec![Operation::WheelRotations(WheelRotations { is_vertical: vertical, rotation_units: units })],
                    Input::Scancode { code, down } => {
                        let s = Scancode::from_u16(code);
                        vec![if down { Operation::KeyPressed(s) } else { Operation::KeyReleased(s) }]
                    }
                    Input::Unicode { ch, down } => vec![if down { Operation::UnicodeKeyPressed(ch) } else { Operation::UnicodeKeyReleased(ch) }],
                    Input::Clipboard(m) => {
                        let Some(c) = stage.get_svc_processor_mut::<CliprdrClient>() else { continue };
                        let msgs = match m {
                            ClipboardMessage::SendInitiateCopy(f) => c.initiate_copy(&f),
                            ClipboardMessage::SendInitiateFileCopy(f) => c.initiate_file_copy(f),
                            ClipboardMessage::SendFormatData(r) => c.submit_format_data(r),
                            ClipboardMessage::SendInitiatePaste(f) => c.initiate_paste(f),
                            ClipboardMessage::SendFileContentsRequest(r) => c.request_file_contents(r),
                            ClipboardMessage::SendFileContentsResponse(r) => c.submit_file_contents(r),
                            ClipboardMessage::Error(e) => {
                                emit(Event::Info(format!("Zwischenablage: {e}")));
                                continue;
                            }
                        }
                        .map_err(err)?;
                        let frame = stage.process_svc_processor_messages(msgs).map_err(err)?;
                        if !frame.is_empty() {
                            writer.write_all(&frame).await.map_err(|e| e.to_string())?;
                        }
                        continue;
                    }
                    Input::Disconnect => continue, // oben behandelt
                    Input::ReleaseAll => {
                        let events = input.release_all();
                        let out = stage.process_fastpath_input(&mut image, &events).map_err(err)?;
                        write_all(&mut writer, &out).await?;
                        continue;
                    }
                    Input::Monitors(list) => {
                        let layout = match monitor_layout(&list) {
                            Ok(l) => l,
                            Err(e) => { emit(Event::Info(e)); continue; }
                        };
                        match stage.encode_monitor_layout(&layout) {
                            Some(frame) => writer.write_all(&frame.map_err(err)?).await.map_err(|e| e.to_string())?,
                            None => { emit(Event::Info("Der Server kann keine Monitor-Anordnung übernehmen.".into())); }
                        }
                        continue;
                    }
                    Input::Resize { width, height, scale } => {
                        let (w, h) = MonitorLayoutEntry::adjust_display_size(u32::from(width), u32::from(height));
                        pending_resize = Some((w, h, scale));
                        send_pending_resize(&mut stage, &mut writer, &mut pending_resize).await?;
                        continue;
                    }
                };
                let events = input.apply(ops);
                stage.process_fastpath_input(&mut image, &events).map_err(err)?
            }
        };

        // Der Anzeigekanal (DISP) öffnet sich erst nach der Anmeldung: wartenden Grössenwunsch jetzt senden.
        if pending_resize.is_some() {
            send_pending_resize(&mut stage, &mut writer, &mut pending_resize).await?;
            if pending_resize.is_some() && !resize_hint && started.elapsed() > std::time::Duration::from_secs(10) {
                resize_hint = true;
                emit(Event::Info("Der Server kann seine Auflösung nicht anpassen; die Anzeige wird skaliert.".into()));
            }
        }
        for out in outputs {
            match out {
                ActiveStageOutput::ResponseFrame(f) => writer.write_all(&f).await.map_err(|e| e.to_string())?,
                ActiveStageOutput::GraphicsUpdate(r) => {
                    // EGFX ResetGraphics (z. B. nach einer Grössenänderung) vergrössert/verkleinert `image` selbst:
                    // neue Grösse zuerst melden, sonst landen die Bildteile in der alten Fläche (schwarz/verschoben).
                    if (image.width(), image.height()) != shown {
                        shown = (image.width(), image.height());
                        tracing::info!(width = shown.0, height = shown.1, "RDP: neue Bildgrösse");
                        if !emit(Event::Resize { width: shown.0, height: shown.1 }) {
                            return Ok(Flow::Closed("Fenster geschlossen".into()));
                        }
                    }
                    if let Some(rect) = rect_of(&r, &image) {
                        if !emit(Event::Rect { rect, rgba: crop(&image, rect) }) {
                            return Ok(Flow::Closed("Fenster geschlossen".into()));
                        }
                    }
                }
                ActiveStageOutput::PointerDefault => {
                    // 0×0 mit hot_x = 1 heisst im Frontend «Standardzeiger».
                    emit(Event::Cursor { hot_x: 1, hot_y: 0, w: 0, h: 0, rgba: Vec::new() });
                }
                ActiveStageOutput::PointerHidden => {
                    emit(Event::Cursor { hot_x: 0, hot_y: 0, w: 0, h: 0, rgba: Vec::new() });
                }
                ActiveStageOutput::PointerBitmap(p) => {
                    emit(Event::Cursor { hot_x: p.hotspot_x, hot_y: p.hotspot_y, w: p.width, h: p.height, rgba: p.bitmap_data.clone() });
                }
                ActiveStageOutput::DeactivateAll => {
                    let mut seq = activation.create();
                    let mut buf = ironrdp_pdu::WriteBuf::new();
                    loop {
                        // Wie `single_sequence_step_read`, aber mit Zugriff auf den Rahmen für den Umleitungs-Rückfall.
                        buf.clear();
                        let written = match seq.next_pdu_hint() {
                            Some(hint) => {
                                let pdu = reader.read_by_hint(hint).await.map_err(|e| format!("Reaktivierung: {e}"))?;
                                match seq.step(&pdu, reader.last_read_at(), &mut buf) {
                                    Ok(w) => w,
                                    Err(e) => {
                                        if let Some(r) = rdp_redir::find(&pdu).filter(useful_redirect) {
                                            tracing::info!(error = %diag::chain(&e), "Umleitung bei Reaktivierung über eigenen Decoder erkannt");
                                            return Ok(Flow::Redirect(r));
                                        }
                                        return Err(phase("Reaktivierung", e));
                                    }
                                }
                            }
                            None => seq.step_no_input(&mut buf).map_err(|e| phase("Reaktivierung", e))?,
                        };
                        if written.size().is_some() {
                            writer.write_all(buf.filled()).await.map_err(|e| e.to_string())?;
                        }
                        match seq.connection_activation_state() {
                            ironrdp_connector::connection_activation::ConnectionActivationState::Redirected { redirection } => {
                                return Ok(Flow::Redirect(Redir::from(&*redirection)));
                            }
                            ironrdp_connector::connection_activation::ConnectionActivationState::Finalized {
                                desktop_size,
                                share_id,
                                enable_server_pointer,
                                pointer_software_rendering,
                                static_channel_chunk_size,
                                ..
                            } => {
                                image = DecodedImage::new(ironrdp_graphics::image_processing::PixelFormat::RgbA32, desktop_size.width, desktop_size.height);
                                shown = (image.width(), image.height());
                                if !stage.reactivate(
                                    seq.io_channel_id(),
                                    seq.user_channel_id(),
                                    share_id,
                                    enable_server_pointer,
                                    pointer_software_rendering,
                                    static_channel_chunk_size,
                                ) {
                                    return Err("Ungültige Kanalgrösse bei der Reaktivierung".into());
                                }
                                if !emit(Event::Resize { width: desktop_size.width, height: desktop_size.height }) {
                                    return Ok(Flow::Closed("Fenster geschlossen".into()));
                                }
                                break;
                            }
                            _ => {}
                        }
                    }
                }
                ActiveStageOutput::ServerRedirection(r) => return Ok(Flow::Redirect(Redir::from(&*r))),
                ActiveStageOutput::Terminate(reason) => return Ok(Flow::Closed(reason.to_string())),
                _ => {}
            }
        }
    }
}

/// Sendet den wartenden Grössenwunsch, sobald der Server den Anzeigekanal (DISP) bereitgestellt hat.
async fn send_pending_resize<W: FramedWrite>(stage: &mut ActiveStage, w: &mut W, pending: &mut Option<(u32, u32, u32)>) -> Result<(), String> {
    let Some((width, height, scale)) = *pending else { return Ok(()) };
    let Some(frame) = stage.encode_resize(width, height, Some(scale), None) else { return Ok(()) };
    *pending = None;
    w.write_all(&frame.map_err(err)?).await.map_err(|e| e.to_string())
}

async fn write_all<W: FramedWrite>(w: &mut W, outs: &[ActiveStageOutput]) -> Result<(), String> {
    for o in outs {
        if let ActiveStageOutput::ResponseFrame(f) = o {
            w.write_all(f).await.map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disconnect_sends_user_requested_ultimatum() {
        use ironrdp_pdu::mcs::{DisconnectProviderUltimatum, DisconnectReason};
        let f = disconnect_frame();
        assert!(!f.is_empty());
        let d = ironrdp_pdu::decode::<ironrdp_pdu::x224::X224<DisconnectProviderUltimatum>>(&f).unwrap().0;
        assert_eq!(d.reason, DisconnectReason::UserRequested);
    }

    #[test]
    fn gnome_connect_time_autodetect() {
        let hex = |h: &str| -> Vec<u8> { (0..h.len() / 2).map(|i| u8::from_str_radix(&h[i * 2..i * 2 + 2], 16).unwrap()).collect() };
        // Aus Mikes Log (GNOME 3389 nach RDSTLS): Bandwidth Measure Start auf Kanal 0.
        let start = hex("0300001902f080680006000070800a00100000060000001410");
        let mut bw = None;
        assert_eq!(connect_time_autodetect(&start, 1007, &mut bw), Some(None), "erkannt, keine Antwort");
        assert!(bw.is_some(), "Messung gestartet");
        assert_eq!(license_flag(&start), Some(false), "früher fälschlich als «Lizenz übersprungen» gewertet");

        // RTT-Anfrage (Sequenz 7, Typ 0x1001) wird beantwortet, auf demselben Kanal.
        let rtt = hex("0300001902f080680006000070800a00100000060007000110");
        let reply = connect_time_autodetect(&rtt, 1007, &mut bw).unwrap().unwrap();
        let ironrdp_pdu::mcs::McsMessage::SendDataRequest(req) = ironrdp_pdu::decode::<ironrdp_pdu::x224::X224<ironrdp_pdu::mcs::McsMessage<'_>>>(&reply).unwrap().0 else {
            panic!("keine Send Data Request")
        };
        assert_eq!((req.initiator_id, req.channel_id), (1007, 0));
        use ironrdp_pdu::rdp::autodetect::{AutoDetectResponse, AutoDetectRspPdu};
        assert_eq!(ironrdp_pdu::decode::<AutoDetectRspPdu>(&req.user_data).unwrap().response, AutoDetectResponse::RttResponse { sequence_number: 7 });

        // Ein Lizenzpaket (SEC_LICENSE_PKT) ist kein Auto-Detect.
        let lic = hex("0300001302f08068000603eb70800480000000");
        assert_eq!(connect_time_autodetect(&lic, 1007, &mut bw), None);
    }

    #[test]
    fn share_names_are_plain() {
        assert_eq!(share_name(std::path::Path::new("C:/Users/mike/Austausch")), "Austausch");
        assert_eq!(share_name(std::path::Path::new("/tmp/a:b*c")), "abc");
        assert_eq!(share_name(std::path::Path::new("/")), "Sesam");
        let dir = std::env::temp_dir();
        assert!(drive_channel(&dir).is_ok());
        assert!(drive_channel(std::path::Path::new("relativ")).is_err());
    }

    #[test]
    fn monitor_layout_is_relative_to_primary() {
        let m = |left, top, width, height, primary| Monitor { left, top, width, height, primary };
        // Zweiter Monitor links vom Hauptmonitor: negative Position ist erlaubt.
        assert!(monitor_layout(&[m(0, 0, 2560, 1440, true), m(-1920, 200, 1920, 1080, false)]).is_ok());
        // Hauptmonitor nicht bei (0,0) im System: wird verschoben.
        assert!(monitor_layout(&[m(1920, 0, 1920, 1080, false), m(3840, 0, 1921, 1080, true)]).is_ok());
        assert!(monitor_layout(&[]).is_err());
    }

    #[test]
    fn license_flag_detects_license_and_other_pdus() {
        // TPKT + X.224 Data + MCS Send Data Indication (Kanal 1003) + Nutzdaten
        let frame = |user: &[u8]| {
            let mut mcs = vec![0x68, 0x00, 0x01, 0x03, 0xEB, 0x70, 0x80 | 0, user.len() as u8];
            mcs.extend_from_slice(user);
            let mut f = vec![0x03, 0x00, 0x00, 0x00, 0x02, 0xF0, 0x80];
            f.extend(mcs);
            let n = f.len() as u16;
            f[2..4].copy_from_slice(&n.to_be_bytes());
            f
        };
        assert_eq!(license_flag(&frame(&[0x80, 0x00, 0x00, 0x00, 0xFF, 0x03])), Some(true));
        assert_eq!(license_flag(&frame(&[0x2A, 0x01, 0x11, 0x00, 0xEA, 0x03])), Some(false), "Demand Active statt Lizenz");
        assert_eq!(license_flag(&[1, 2, 3]), None);
    }

    #[test]
    fn scancodes() {
        assert_eq!(scancode("KeyA"), Some(0x1E));
        assert_eq!(scancode("KeyZ"), Some(0x2C));
        assert_eq!(scancode("Digit1"), Some(0x02));
        assert_eq!(scancode("Digit0"), Some(0x0B));
        assert_eq!(scancode("F1"), Some(0x3B));
        assert_eq!(scancode("F12"), Some(0x58));
        assert_eq!(scancode("Delete"), Some(0xE053));
        assert_eq!(scancode("KeyAB"), None);
        assert_eq!(scancode("F13"), None);
        assert_eq!(scancode("Lang1"), None);
    }

    #[test]
    fn altgr_characters_go_as_unicode() {
        assert!(matches!(key_input("Digit2", "@", true, true), Some(Input::Unicode { ch: '@', down: true })));
        assert!(matches!(key_input("KeyC", "c", true, false), Some(Input::Scancode { code: 0x2E, .. })));
        assert!(key_input("AltRight", "AltGraph", true, false).is_none());
        assert!(matches!(key_input("", "€", true, false), Some(Input::Unicode { ch: '€', .. })));
        assert!(key_input("", "Dead", true, false).is_none());
    }

    #[test]
    fn redirect_updates_target() {
        let mut t = Target {
            host: "doerig-02".into(),
            port: 3389,
            user: "mike".into(),
            domain: None,
            password: "alt".into(),
            expected_cert: None,
            skip_cert_check: false,
            width: 800,
            height: 600,
            scale: 100,
            clipboard: None,
            audio: false,
            share: None,
            kdc: None,
            no_ntlm: false,
            gateway: None,
        };
        let mut hop = Hop::default();
        let pw: Vec<u8> = "einmal".encode_utf16().chain([0]).flat_map(u16::to_le_bytes).collect();
        let r = Redir {
            session_id: 7,
            load_balance_info: Some(b"Cookie: msts=abc\r\n".to_vec()),
            username: Some("mike".into()),
            password: Some(pw.clone()),
            target_cert: Some(vec![0x30, 1, 2]),
            ..Default::default()
        };
        apply_redirect(&mut t, &mut hop, &r).unwrap();
        assert_eq!(hop.session_id, 7);
        assert_eq!(hop.routing_token.as_deref(), Some("Cookie: msts=abc"));
        assert_eq!(hop.host, None, "ohne Zieladresse derselbe Host");
        assert_eq!(hop.cookie, Some(pw));
        assert_eq!(t.password, "einmal");
        assert_eq!(t.expected_cert, Some(fingerprint(&[0x30, 1, 2])), "Ziel-Zertifikat gilt ohne Rückfrage");
        assert!(hop.rdstls.is_none());

        // GNOME-Übergabe: verschlüsseltes Passwort → RDSTLS, Blob und GUID unverändert, kein NLA-Passwort.
        let r = Redir {
            session_id: 9,
            load_balance_info: Some(b"Cookie: msts=xyz\r\n".to_vec()),
            username: Some("mike".into()),
            password: Some(vec![9, 9, 9]),
            password_pk_encrypted: true,
            redirection_guid: Some(vec![7, 7]),
            ..Default::default()
        };
        apply_redirect(&mut t, &mut hop, &r).unwrap();
        let c = hop.rdstls.clone().unwrap();
        assert_eq!((c.password_blob, c.redirection_guid, c.user.as_str()), (vec![9, 9, 9], vec![7, 7], "mike"));
        assert_eq!(hop.cookie, None);
        assert_eq!(t.password, "einmal", "verschlüsseltes Passwort ersetzt nichts");

        let bad = Redir { load_balance_info: Some(vec![1, 2, 3]), ..Default::default() };
        assert!(apply_redirect(&mut t, &mut hop, &bad).is_err());
    }
}

/// Probe gegen einen echten Server, ohne Zugangsdaten (falsches Passwort):
/// `SESAM_RDP_PROBE=192.168.1.4:3389 cargo test --lib probe -- --ignored --nocapture`
#[cfg(test)]
mod probe {
    use super::*;

    /// Prüft nur die eigene X.224-Aushandlung mit RDSTLS gegen einen echten Server.
    #[tokio::test(flavor = "multi_thread")]
    #[ignore]
    async fn probe_rdstls_negotiation() {
        let Ok(addr) = std::env::var("SESAM_RDP_PROBE") else { return };
        let tcp = Stream::Tcp(TcpStream::connect(addr.as_str()).await.unwrap());
        let mut framed = TokioFramed::new(tcp);
        let hop = Hop { routing_token: Some("Cookie: msts=sesam-probe".into()), ..Default::default() };
        println!("RDSTLS-Aushandlung: {:?}", rdstls_negotiate(&mut framed, &hop).await);
    }

    /// Anmeldung wie ein Mensch: am GNOME-Anmeldebildschirm Benutzer und Passwort tippen, Umleitung abwarten,
    /// Bildschirmfotos nach test-build/diag/. Zugangsdaten nur aus dem Tresor (Einstellungen › Zugangsdaten):
    /// «Sesam Test RDP» (RDP-Anmeldung) und, falls `SESAM_RDP_DESKTOP` nicht gesetzt, «Sesam Test» (Ubuntu-Konto).
    /// `SESAM_RDP_LOGIN=192.168.1.4:3389 cargo test --lib probe_login -- --ignored --nocapture`
    /// Mit `SESAM_RDP_DESKTOP=1` nur verbinden und Bilder speichern (direkte Freigabe, z. B. Port 3392, mit «Sesam Test»).
    #[tokio::test(flavor = "multi_thread")]
    #[ignore]
    async fn probe_login() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let Ok(addr) = std::env::var("SESAM_RDP_LOGIN") else { return };
        let desktop_only = std::env::var("SESAM_RDP_DESKTOP").is_ok();
        crate::diag::init(&std::env::temp_dir().join("sesam-probe"));
        let appdata = std::path::PathBuf::from(std::env::var("APPDATA").unwrap()).join("ch.sesam.remote");
        let creds = crate::creds::load(&appdata.join("credentials.json")).expect("Zugangsdaten-Liste");
        let get = |name: &str| {
            let c = creds.iter().find(|c| c.name == name).unwrap_or_else(|| panic!("Zugangsdaten «{name}» fehlen"));
            let pw = crate::vault::get(&crate::creds::vault_id(&c.id), crate::vault::Kind::Password).unwrap().expect("Passwort im Tresor");
            crate::diag::add_secret(&pw);
            (c.user.clone(), pw)
        };
        let (user, password) = get(if desktop_only { "Sesam Test" } else { "Sesam Test RDP" });
        let (host, port) = addr.rsplit_once(':').unwrap();
        let pins: std::collections::HashMap<String, String> =
            std::fs::read_to_string(appdata.join("known_certs.json")).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
        let t = Target {
            host: host.into(),
            port: port.parse().unwrap(),
            user,
            domain: None,
            password,
            expected_cert: pins.get(&addr).cloned(),
            skip_cert_check: false,
            width: 1280,
            height: 800,
            scale: 100,
            clipboard: None,
            audio: false,
            share: None,
            kdc: None,
            no_ntlm: false,
            gateway: None,
        };
        // Grösse wie die App sie anfragt, z. B. SESAM_RDP_SIZE=720x1280 (Hochformat).
        let mut t = t;
        if let Some((w, h)) = std::env::var("SESAM_RDP_SIZE").ok().and_then(|v| v.split_once('x').and_then(|(a, b)| Some((a.parse().ok()?, b.parse().ok()?)))) {
            (t.width, t.height) = (w, h);
        }
        println!("Zertifikat gepinnt: {}", t.expected_cert.is_some());
        // Bild mitschreiben wie die Anzeige.
        let fb = std::sync::Arc::new(std::sync::Mutex::new((0u16, 0u16, Vec::<u8>::new())));
        let rects = std::sync::Arc::new(AtomicUsize::new(0));
        let (fb2, rects2) = (fb.clone(), rects.clone());
        let emit = move |e: Event| {
            match e {
                Event::Resize { width, height } => *fb2.lock().unwrap() = (width, height, vec![0; width as usize * height as usize * 4]),
                Event::Rect { rect, rgba } => {
                    let mut g = fb2.lock().unwrap();
                    let w = g.0 as usize;
                    for row in 0..rect.h as usize {
                        let dst = ((rect.y as usize + row) * w + rect.x as usize) * 4;
                        let src = row * rect.w as usize * 4;
                        if dst + rect.w as usize * 4 <= g.2.len() {
                            g.2[dst..dst + rect.w as usize * 4].copy_from_slice(&rgba[src..src + rect.w as usize * 4]);
                        }
                    }
                    rects2.fetch_add(1, Ordering::Relaxed);
                }
                Event::Info(i) if !i.is_empty() => println!("Hinweis: {i}"),
                Event::Closed(why) => println!("Beendet: {why}"),
                _ => {}
            }
            true
        };
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../test-build/diag");
        std::fs::create_dir_all(&dir).unwrap();
        let snap = |name: &str| {
            let g = fb.lock().unwrap();
            if g.0 == 0 {
                return println!("{name}: noch kein Bild");
            }
            let f = std::fs::File::create(dir.join(format!("{name}.png"))).unwrap();
            let mut enc = png::Encoder::new(std::io::BufWriter::new(f), g.0 as u32, g.1 as u32);
            enc.set_color(png::ColorType::Rgba);
            enc.write_header().unwrap().write_image_data(&g.2).unwrap();
            println!("{name}: gespeichert ({}x{}, {} Rechtecke bisher)", g.0, g.1, rects.load(Ordering::Relaxed));
        };
        let wait = |s: u64| tokio::time::sleep(std::time::Duration::from_secs(s));
        let session = match open(t, Default::default(), channel(), emit).await {
            Ok(s) => s,
            Err(e) => return println!("Verbindung fehlgeschlagen: {e}"),
        };
        // Wie die App: Grösse sofort nach dem Verbinden nachschieben (SESAM_RDP_RESIZE_NOW=WxH).
        if let Some((w, h)) = std::env::var("SESAM_RDP_RESIZE_NOW").ok().and_then(|v| v.split_once('x').and_then(|(a, b)| Some((a.parse().ok()?, b.parse().ok()?)))) {
            tokio::time::sleep(std::time::Duration::from_millis(std::env::var("SESAM_RDP_RESIZE_MS").ok().and_then(|v| v.parse().ok()).unwrap_or(1500))).await;
            session.send(Input::Resize { width: w, height: h, scale: 100 });
        }
        for _ in 0..30 {
            if rects.load(Ordering::Relaxed) > 0 {
                break;
            }
            wait(1).await;
        }
        wait(3).await;
        snap("1-start");
        // Grösse mitten in der Sitzung ändern wie die App (DisplayControl), z. B. SESAM_RDP_RESIZE=300x1296.
        if let Some((w, h)) = std::env::var("SESAM_RDP_RESIZE").ok().and_then(|v| v.split_once('x').and_then(|(a, b)| Some((a.parse().ok()?, b.parse().ok()?)))) {
            let before = rects.load(Ordering::Relaxed);
            session.send(Input::Resize { width: w, height: h, scale: 100 });
            wait(4).await;
            println!("Nach Grössenänderung: {} neue Rechtecke", rects.load(Ordering::Relaxed) - before);
            snap("2-nach-groessenaenderung");
        }
        if !desktop_only {
            let (os_user, os_pw) = get("Sesam Test");
            let typ = |text: &str| {
                for ch in text.chars() {
                    session.send(Input::Unicode { ch, down: true });
                    session.send(Input::Unicode { ch, down: false });
                }
            };
            let enter = || {
                session.send(Input::Scancode { code: 0x1C, down: true });
                session.send(Input::Scancode { code: 0x1C, down: false });
            };
            // GDM: Eintrag des Testkontos anklicken (Tippen springt nicht dorthin, Enter nähme den ersten Benutzer).
            // Position aus dem ersten Bild, überschreibbar mit SESAM_RDP_CLICK="x,y".
            let (x, y) = std::env::var("SESAM_RDP_CLICK")
                .ok()
                .and_then(|v| v.split_once(',').and_then(|(a, b)| Some((a.parse().ok()?, b.parse().ok()?))))
                .unwrap_or((640u16, 443u16));
            let _ = &os_user;
            session.send(Input::Pointer { x, y, buttons: 0 });
            session.send(Input::Pointer { x, y, buttons: 1 });
            session.send(Input::Pointer { x, y, buttons: 0 });
            wait(3).await;
            snap("3-passwortfeld");
            if std::env::var("SESAM_RDP_STOP").is_ok() {
                session.close(); // nur bis zum Passwortfeld, zum Prüfen des richtigen Kontos
                return wait(1).await;
            }
            typ(&os_pw);
            enter();
        }
        for i in 0..10 {
            wait(3).await;
            snap(&format!("4-danach-{:02}", i * 3));
        }
        session.close();
        wait(1).await; // Disconnect noch senden lassen
    }

    /// «Zugangsdaten prüfen» gegen einen echten Server, mit «Sesam Test RDP» aus dem Tresor; mit
    /// `SESAM_RDP_WRONG=1` absichtlich falsches Passwort (erwartet: Anmeldung abgelehnt).
    /// `SESAM_RDP_CHECK=192.168.1.4:3389 cargo test --lib probe_check -- --ignored --nocapture`
    #[tokio::test(flavor = "multi_thread")]
    #[ignore]
    async fn probe_check() {
        let Ok(addr) = std::env::var("SESAM_RDP_CHECK") else { return };
        let appdata = std::path::PathBuf::from(std::env::var("APPDATA").unwrap()).join("ch.sesam.remote");
        let creds = crate::creds::load(&appdata.join("credentials.json")).unwrap();
        // Zugangsdaten aus SESAM_RDP_USER/SESAM_RDP_PASS (nur für diesen Lauf), sonst «Sesam Test RDP» aus dem Tresor.
        let (user, mut pw) = match (std::env::var("SESAM_RDP_USER"), std::env::var("SESAM_RDP_PASS")) {
            (Ok(u), Ok(p)) => (u, p),
            _ => {
                let c = creds.iter().find(|c| c.name == "Sesam Test RDP").expect("«Sesam Test RDP» fehlt");
                (c.user.clone(), crate::vault::get(&crate::creds::vault_id(&c.id), crate::vault::Kind::Password).unwrap().unwrap_or_default())
            }
        };
        let domain = std::env::var("SESAM_RDP_DOMAIN").ok();
        if std::env::var("SESAM_RDP_WRONG").is_ok() {
            pw = "absichtlich-falsch".into();
        }
        let pins: std::collections::HashMap<String, String> =
            std::fs::read_to_string(appdata.join("known_certs.json")).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
        let (host, port) = addr.rsplit_once(':').unwrap();
        let t = Target {
            host: host.into(), port: port.parse().unwrap(), user, domain, password: pw,
            expected_cert: std::env::var("SESAM_RDP_TRUST").ok().or_else(|| pins.get(&addr).cloned()), skip_cert_check: false, width: 1024, height: 768, scale: 100,
            clipboard: None, audio: false, share: None, kdc: None, no_ntlm: false, gateway: None,
        };
        println!("Ergebnis: {:?}", check_login(t, Default::default()).await);
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore]
    async fn probe_real_server() {
        let Ok(addr) = std::env::var("SESAM_RDP_PROBE") else { return };
        let (host, port) = addr.rsplit_once(':').unwrap();
        let mut t = Target {
            host: host.into(),
            port: port.parse().unwrap(),
            user: "sesam-probe".into(),
            domain: None,
            password: "absichtlich-falsch".into(),
            expected_cert: None,
            skip_cert_check: false,
            width: 1280,
            height: 800,
            scale: 100,
            clipboard: None,
            audio: false,
            share: None,
            kdc: None,
            no_ntlm: false,
            gateway: None,
        };
        for round in 1..=2 {
            let seen: SeenCert = Default::default();
            let r = open(t.clone(), seen.clone(), channel(), |_| true).await;
            match r {
                Ok(_) => println!("Runde {round}: verbunden (unerwartet ohne Zugangsdaten)"),
                Err(e) => println!("Runde {round}: {e}"),
            }
            match seen.lock().unwrap().clone() {
                Some(fp) => t.expected_cert = Some(fp),
                None => break,
            }
        }
    }
}
