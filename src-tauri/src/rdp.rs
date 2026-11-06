//! RDP über IronRDP: TLS mit Zertifikat-Pinning, NLA (CredSSP/NTLM), Graphics Pipeline (EGFX),
//! Display Control (Auflösung folgt dem Fenster) und Server-Redirection (GNOME-Übergabe nach dem Login).
//! Bild und Ereignisse gehen über dieselben `vnc::Event`s wie bei VNC, damit die Anzeige gleich bleibt.

use crate::vnc::Event;
use crate::vnc::decode::Rect;
use crate::vnc::tls::{CERT_CHANGED, CERT_UNKNOWN, fingerprint};
use ironrdp_cliprdr::backend::{ClipboardMessage, ClipboardMessageProxy, CliprdrBackendFactory};
use ironrdp_cliprdr::CliprdrClient;
use ironrdp_connector::sspi::generator::NetworkRequest;
use ironrdp_connector::{ClientConnector, ConnectionOutcome, ConnectionResult, ConnectorResult, Credentials, DesktopSize};
use ironrdp_displaycontrol::client::DisplayControlClient;
use ironrdp_displaycontrol::pdu::MonitorLayoutEntry;
use ironrdp_egfx::client::{GraphicsPipelineClient, GraphicsPipelineHandler};
use ironrdp_input::{Database, MouseButton, MousePosition, Operation, Scancode, WheelRotations};
use ironrdp_pdu::gcc;
use ironrdp_pdu::geometry::InclusiveRectangle;
use ironrdp_pdu::rdp::server_redirection::{ServerRedirectionFlags, ServerRedirectionPdu};
use ironrdp_session::image::DecodedImage;
use ironrdp_session::{ActiveStage, ActiveStageBuilder, ActiveStageOutput};
use ironrdp_tokio::{FramedWrite, TokioFramed, single_sequence_step_read, split_tokio_framed};
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
    pub width: u16,
    pub height: u16,
    /// Zwischenablage (cliprdr); für jede Verbindung, auch nach Umleitungen, ein neues Backend.
    pub clipboard: Option<ClipFactory>,
    /// Ton des entfernten Rechners hier abspielen.
    pub audio: bool,
    /// Lokaler Ordner, der auf dem Server als Laufwerk erscheint.
    pub share: Option<std::path::PathBuf>,
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
    Resize { width: u16, height: u16 },
    ReleaseAll,
    /// Vom Zwischenablage-Backend.
    Clipboard(ClipboardMessage),
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
    pub fn close(&self) {
        self.task.abort();
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

/// Kerberos braucht einen KDC-Zugang; wir melden uns per NTLM an.
struct NoKdc;

impl ironrdp_tokio::NetworkClient for NoKdc {
    async fn send(&mut self, _: &NetworkRequest) -> ConnectorResult<Vec<u8>> {
        Err(ironrdp_connector::general_err!("Kerberos wird nicht unterstützt"))
    }
}

/// Was eine Server-Redirection für den nächsten Versuch ändert.
#[derive(Default, Clone)]
struct Hop {
    host: Option<String>,
    session_id: u32,
    routing_token: Option<String>,
    cookie: Option<Vec<u8>>,
}

fn utf16_text(b: &[u8]) -> Option<String> {
    if b.len() % 2 != 0 {
        return None;
    }
    let units: Vec<u16> = b.chunks_exact(2).map(|p| u16::from_le_bytes([p[0], p[1]])).take_while(|u| *u != 0).collect();
    String::from_utf16(&units).ok().filter(|s| !s.is_empty() && !s.chars().any(char::is_control))
}

/// Ziel und Anmeldung für den nächsten Versuch anpassen ([MS-RDPBCGR] 3.2.5.13.1).
fn apply_redirect(t: &mut Target, hop: &mut Hop, r: &ServerRedirectionPdu) -> Result<(), String> {
    if r.flags.contains(ServerRedirectionFlags::PASSWORD_IS_PK_ENCRYPTED) && r.password.is_some() {
        return Err("Der Server leitet mit verschlüsseltem Passwort (RDSTLS) um; das wird nicht unterstützt".into());
    }
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
        .chain(r.target_net_addresses.iter().flatten())
        .chain(r.target_fqdn.iter())
        .chain(r.target_netbios_name.iter())
        .find(|h| !h.is_empty())
        .cloned();
    if let Some(u) = r.username.as_ref().filter(|u| !u.is_empty()) {
        t.user = u.clone();
    }
    if let Some(d) = &r.domain {
        t.domain = (!d.is_empty()).then(|| d.clone());
    }
    // GNOME (und FreeRDP) nutzen das Passwort aus der Umleitung auch für NLA.
    if let Some(pw) = r.password.as_deref().and_then(utf16_text) {
        t.password = pw;
    }
    *hop = Hop { host, session_id: r.session_id, routing_token, cookie: r.password.clone() };
    Ok(())
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
        desktop_scale_factor: 0,
        desktop_physical_size: None,
        desktop_orientation: None,
        device_scale_factor: None,
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

type Framed = TokioFramed<ironrdp_tls::TlsStream<TcpStream>>;

/// Ein Verbindungsversuch bis zum Ende des Verbindungsaufbaus.
async fn connect_once(t: &Target, hop: &Hop, seen: &SeenCert) -> Result<(ConnectionOutcome, Framed), String> {
    let host = hop.host.clone().unwrap_or_else(|| t.host.clone());
    let tcp = tokio::time::timeout(std::time::Duration::from_secs(15), TcpStream::connect((host.as_str(), t.port)))
        .await
        .map_err(|_| "Zeitüberschreitung beim Verbinden".to_string())?
        .map_err(|e| format!("Verbindung fehlgeschlagen: {e}"))?;
    let _ = tcp.set_nodelay(true);
    let client_addr = tcp.local_addr().map_err(|e| e.to_string())?;

    let mut drdynvc = ironrdp_dvc::DrdynvcClient::new()
        .with_dynamic_channel(DisplayControlClient::new(|_| Ok(Vec::new())))
        .with_dynamic_channel(GraphicsPipelineClient::new(Box::new(NoGfxCallbacks), None));
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
    let should_upgrade = ironrdp_tokio::connect_begin(&mut framed, &mut connector).await.map_err(err)?;
    let (tcp, leftover) = framed.into_inner();

    // Zertifikate mit gültiger Kette akzeptiert rustls selbst; sonst entscheidet das Pinning.
    let expected = t.expected_cert.clone();
    let seen2 = seen.clone();
    let callback: ironrdp_tls::CertificateValidationCallback = Arc::new(move |der: &[u8], _: &str, _: &str| {
        let fp = fingerprint(der);
        let ok = expected.as_deref() == Some(fp.as_str());
        *seen2.lock().unwrap() = Some(fp);
        ok
    });
    let (tls, cert) = ironrdp_tls::upgrade_with_certificate_validation_callback(tcp, &t.host, callback).await.map_err(|e| {
        match (seen.lock().unwrap().clone(), &t.expected_cert) {
            (Some(fp), None) => format!("{CERT_UNKNOWN}{fp}"),
            (Some(fp), Some(want)) if &fp != want => format!("{CERT_CHANGED}{fp}"),
            _ => format!("TLS-Verbindung fehlgeschlagen: {e}"),
        }
    })?;
    let upgraded = ironrdp_tokio::mark_as_upgraded(should_upgrade, &mut connector);
    let key = ironrdp_tls::extract_tls_server_public_key(&cert).ok_or("Öffentlicher Schlüssel des Servers fehlt")?.to_vec();
    let mut framed = TokioFramed::new_with_leftover(tls, leftover);
    let outcome = ironrdp_tokio::connect_finalize(
        upgraded,
        connector,
        &mut framed,
        &mut NoKdc,
        ironrdp_connector::ServerName::new(t.host.clone()),
        key,
        None,
    )
    .await
    .map_err(err)?;
    Ok((outcome, framed))
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

fn err(e: impl std::fmt::Display) -> String {
    let s = e.to_string();
    if s.contains("LogonFailure") || s.contains("logon failure") || s.contains("SEC_E_LOGON_DENIED") {
        "Anmeldung abgelehnt (Benutzer, Passwort oder Domäne prüfen)".into()
    } else {
        format!("RDP: {s}")
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
                apply_redirect(&mut t, &mut hop, &r)?;
            }
        }
    };
    let task = tokio::spawn(run(t, hop, seen, result, framed, rx, emit, redirects));
    Ok(Session { tx, task })
}

enum Flow {
    Closed(String),
    Redirect(Box<ServerRedirectionPdu>),
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
                apply_redirect(t, hop, &r)?;
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
                let (action, payload) = frame.map_err(|e| format!("Verbindung beendet: {e}"))?;
                stage.process(&mut image, action, &payload, reader.last_read_at()).map_err(err)?
            }
            msg = rx.recv() => {
                let Some(msg) = msg else { return Ok(Flow::Closed("Fenster geschlossen".into())) };
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
                    Input::ReleaseAll => {
                        let events = input.release_all();
                        let out = stage.process_fastpath_input(&mut image, &events).map_err(err)?;
                        write_all(&mut writer, &out).await?;
                        continue;
                    }
                    Input::Resize { width, height } => {
                        let (w, h) = MonitorLayoutEntry::adjust_display_size(u32::from(width), u32::from(height));
                        match stage.encode_resize(w, h, None, None) {
                            Some(frame) => {
                                let frame = frame.map_err(err)?;
                                writer.write_all(&frame).await.map_err(|e| e.to_string())?;
                            }
                            None => { emit(Event::Info("Der Server kann seine Auflösung nicht anpassen; die Anzeige wird skaliert.".into())); }
                        }
                        continue;
                    }
                };
                let events = input.apply(ops);
                stage.process_fastpath_input(&mut image, &events).map_err(err)?
            }
        };

        for out in outputs {
            match out {
                ActiveStageOutput::ResponseFrame(f) => writer.write_all(&f).await.map_err(|e| e.to_string())?,
                ActiveStageOutput::GraphicsUpdate(r) => {
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
                        let written = single_sequence_step_read(&mut reader, &mut seq, &mut buf).await.map_err(err)?;
                        if written.size().is_some() {
                            writer.write_all(buf.filled()).await.map_err(|e| e.to_string())?;
                        }
                        match seq.connection_activation_state() {
                            ironrdp_connector::connection_activation::ConnectionActivationState::Redirected { redirection } => {
                                return Ok(Flow::Redirect(redirection));
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
                ActiveStageOutput::ServerRedirection(r) => return Ok(Flow::Redirect(r)),
                ActiveStageOutput::Terminate(reason) => return Ok(Flow::Closed(reason.to_string())),
                _ => {}
            }
        }
    }
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
    fn share_names_are_plain() {
        assert_eq!(share_name(std::path::Path::new("C:/Users/mike/Austausch")), "Austausch");
        assert_eq!(share_name(std::path::Path::new("/tmp/a:b*c")), "abc");
        assert_eq!(share_name(std::path::Path::new("/")), "Sesam");
        let dir = std::env::temp_dir();
        assert!(drive_channel(&dir).is_ok());
        assert!(drive_channel(std::path::Path::new("relativ")).is_err());
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
            width: 800,
            height: 600,
            clipboard: None,
            audio: false,
            share: None,
        };
        let mut hop = Hop::default();
        let pw: Vec<u8> = "einmal".encode_utf16().chain([0]).flat_map(u16::to_le_bytes).collect();
        let r = ServerRedirectionPdu {
            session_id: 7,
            load_balance_info: Some(b"Cookie: msts=abc\r\n".to_vec()),
            username: Some("mike".into()),
            password: Some(pw.clone()),
            ..Default::default()
        };
        apply_redirect(&mut t, &mut hop, &r).unwrap();
        assert_eq!(hop.session_id, 7);
        assert_eq!(hop.routing_token.as_deref(), Some("Cookie: msts=abc"));
        assert_eq!(hop.host, None, "ohne Zieladresse derselbe Host");
        assert_eq!(hop.cookie, Some(pw));
        assert_eq!(t.password, "einmal");

        let bad = ServerRedirectionPdu { load_balance_info: Some(vec![1, 2, 3]), ..Default::default() };
        assert!(apply_redirect(&mut t, &mut hop, &bad).is_err());
    }
}
