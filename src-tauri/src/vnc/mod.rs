//! Eigener RFB-Client (RFC 6143) mit Apple-Auth, ExtendedDesktopSize und Extended Clipboard.
//! Begründung des Entscheids gegen `vnc-rs`: docs/architektur.md.

pub mod auth;
pub mod clipboard;
pub mod decode;
pub mod tls;

use decode::{Framebuffer, Rect, Tight, TightNeed, Zrle, add_dirty};
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::sync::mpsc::{UnboundedSender, unbounded_channel};

const ENC_RAW: i32 = 0;
const ENC_COPYRECT: i32 = 1;
const ENC_TIGHT: i32 = 7;
const ENC_ZRLE: i32 = 16;
/// Tight: JPEG-Qualität 8 von 9 (nur wo ein JPEG-Decoder da ist) und Kompressionsstufe 2 (schnell).
const ENC_JPEG_QUALITY_8: i32 = -24;
const ENC_COMPRESS_2: i32 = -254;
const ENC_DESKTOP_SIZE: i32 = -223;
const ENC_LAST_RECT: i32 = -224;
const ENC_EXT_DESKTOP_SIZE: i32 = -308;
const ENC_CURSOR: i32 = -239;
const ENC_EXT_CLIPBOARD: i32 = 0xC0A1E5CEu32 as i32;

/// Fehler-Anfang: Server bietet nur RealVNC RA2/RA2ne an.
pub const VNC_RA2: &str = "VNC_RA2:";

/// Grenze für Längenangaben vom Server (Namen, Fehlertexte, Raw-Daten).
const MAX_LEN: u32 = 64 << 20;

#[derive(Debug, PartialEq)]
pub enum Event {
    Resize { width: u16, height: u16 },
    /// Alle geänderten Rechtecke eines Bilds in einer Nachricht; die Oberfläche bestätigt es mit `ack`.
    Frame(Vec<(Rect, Vec<u8>)>),
    Clipboard(String),
    Bell,
    /// Lokaler Mauszeiger: Hotspot, Grösse, RGBA (0×0 = unsichtbar).
    Cursor { hot_x: u16, hot_y: u16, w: u16, h: u16, rgba: Vec<u8> },
    /// Hinweis für die Statuszeile, z. B. abgelehnte Grössenänderung.
    Info(String),
    Closed(String),
}

#[derive(Debug, PartialEq)]
pub struct ServerInit {
    pub width: u16,
    pub height: u16,
    pub name: String,
}

// ---------- Handshake ----------

async fn read_reason<S: AsyncRead + Unpin>(s: &mut S) -> String {
    let Ok(len) = s.read_u32().await else { return "Verbindung abgelehnt".into() };
    let mut b = vec![0; len.min(4096) as usize];
    match s.read_exact(&mut b).await {
        Ok(_) => String::from_utf8_lossy(&b).into_owned(),
        Err(_) => "Verbindung abgelehnt".into(),
    }
}

fn security_name(t: u8) -> String {
    match t {
        1 => "Keine".into(),
        2 => "VNC-Passwort".into(),
        5 | 6 => "RA2".into(),
        16 => "Tight".into(),
        18 => "TLS".into(),
        19 => "VeNCrypt (TLS)".into(),
        30 => "Apple".into(),
        129 => "RSA-AES".into(),
        t => format!("Typ {t}"),
    }
}

pub trait Io: AsyncRead + AsyncWrite + Send + Unpin {}
impl<T: AsyncRead + AsyncWrite + Send + Unpin> Io for T {}
/// Klartext- oder TLS-Verbindung, je nach Anmeldeverfahren.
pub type Stream = Box<dyn Io>;

pub struct Login<'a> {
    pub host: &'a str,
    pub user: &'a str,
    /// Kontopasswort (Apple-Auth, VeNCrypt X509Plain).
    pub password: Option<&'a str>,
    /// Eigenes VNC-Passwort (VNC-Auth); fehlt es, gilt das Kontopasswort.
    pub vnc_password: Option<&'a str>,
    /// Zertifikat-Pinning für VeNCrypt/TLS.
    pub cert: Arc<tls::Pinned>,
}

fn io(e: std::io::Error) -> String {
    format!("Verbindung unterbrochen: {e}")
}

async fn vnc_auth(s: &mut Stream, pw: Option<&str>) -> Result<(), String> {
    let mut ch = [0u8; 16];
    s.read_exact(&mut ch).await.map_err(io)?;
    let pw = pw.ok_or("Der Server verlangt ein VNC-Passwort")?;
    s.write_all(&auth::vnc_response(pw.as_bytes(), &ch)).await.map_err(io)
}

/// VeNCrypt: Untertyp aushandeln, TLS starten, darin anmelden.
async fn vencrypt(mut s: Stream, l: &Login<'_>, pw: Option<&str>, vpw: Option<&str>) -> Result<Stream, String> {
    let mut v = [0u8; 2];
    s.read_exact(&mut v).await.map_err(io)?;
    if v[0] != 0 || v[1] < 2 {
        return Err(format!("VeNCrypt-Version {}.{} wird nicht unterstützt", v[0], v[1]));
    }
    s.write_all(&[0, 2]).await.map_err(io)?;
    if s.read_u8().await.map_err(io)? != 0 {
        return Err("Server lehnt VeNCrypt 0.2 ab".into());
    }
    let n = s.read_u8().await.map_err(io)?;
    let mut subs = Vec::with_capacity(n as usize);
    for _ in 0..n {
        subs.push(s.read_u32().await.map_err(io)?);
    }
    // X509Plain, X509Vnc, X509None. Klartext (256) und anonymes TLS (257–259) bieten wir bewusst nicht an.
    let pick = [262u32, 261, 260].into_iter().find(|t| {
        subs.contains(t) && match t {
            262 => pw.is_some(),
            261 => vpw.is_some(),
            _ => true,
        }
    });
    let Some(sub) = pick else {
        return Err(if vpw.is_none() && (subs.contains(&262) || subs.contains(&261)) {
            "Der Server verlangt ein Passwort. Bitte in der Verbindung hinterlegen.".into()
        } else {
            format!("Kein unterstütztes VeNCrypt-Verfahren (Server bietet: {subs:?}; nötig: X509 mit Zertifikat)")
        });
    };
    s.write_u32(sub).await.map_err(io)?;
    if s.read_u8().await.map_err(io)? != 1 {
        return Err("Server lehnt das VeNCrypt-Verfahren ab".into());
    }
    let connector = tokio_rustls::TlsConnector::from(Arc::new(l.cert.config()?));
    let tls = connector.connect(tls::server_name(l.host)?, s).await.map_err(|e| l.cert.explain(e))?;
    let mut s: Stream = Box::new(tls);
    match sub {
        262 => {
            let (u, p) = (l.user.as_bytes(), pw.unwrap_or_default().as_bytes());
            let mut m = (u.len() as u32).to_be_bytes().to_vec();
            m.extend((p.len() as u32).to_be_bytes());
            m.extend_from_slice(u);
            m.extend_from_slice(p);
            s.write_all(&m).await.map_err(io)?;
        }
        261 => vnc_auth(&mut s, vpw).await?,
        _ => {}
    }
    Ok(s)
}

/// Version aushandeln, anmelden, ClientInit/ServerInit, Pixelformat und Kodierungen setzen.
pub async fn handshake(stream: impl Io + 'static, l: &Login<'_>) -> Result<(Stream, ServerInit), String> {
    let mut s: Stream = Box::new(stream);
    let mut v = [0u8; 12];
    s.read_exact(&mut v).await.map_err(io)?;
    let v = std::str::from_utf8(&v).ok().filter(|v| v.starts_with("RFB ") && v.ends_with('\n')).ok_or("Kein VNC-Server")?;
    let minor: u32 = v[8..11].parse().map_err(|_| "Unbekannte RFB-Version")?;
    // macOS meldet 003.889; das ist 3.8 mit Apple-Erweiterungen.
    let minor = if minor >= 8 { 8 } else if minor == 7 { 7 } else { 3 };
    s.write_all(format!("RFB 003.00{minor}\n").as_bytes()).await.map_err(io)?;

    let pw = l.password.filter(|p| !p.is_empty());
    let vpw = l.vnc_password.filter(|p| !p.is_empty()).or(pw);
    let chosen = if minor == 3 {
        let t = s.read_u32().await.map_err(io)?;
        if t == 0 {
            return Err(read_reason(&mut s).await);
        }
        u8::try_from(t).map_err(|_| "Unbekanntes Anmeldeverfahren")?
    } else {
        let n = s.read_u8().await.map_err(io)?;
        if n == 0 {
            return Err(read_reason(&mut s).await);
        }
        let mut types = vec![0; n as usize];
        s.read_exact(&mut types).await.map_err(io)?;
        // Verschlüsselt (VeNCrypt) zuerst, dann Apple, VNC-Passwort, ohne Anmeldung.
        let pick = [19u8, 30, 2, 1].into_iter().find(|t| {
            types.contains(t) && match t {
                30 => pw.is_some() && !l.user.is_empty(),
                2 => vpw.is_some(),
                _ => true,
            }
        });
        let Some(t) = pick else {
            let offered = types.iter().map(|&t| security_name(t)).collect::<Vec<_>>().join(", ");
            return Err(if vpw.is_none() && (types.contains(&2) || types.contains(&30)) {
                "Der Server verlangt ein Passwort. Bitte in der Verbindung hinterlegen.".into()
            } else if types.iter().any(|t| matches!(t, 5 | 6)) {
                // RealVNC RA2/RA2ne: nicht offengelegt; die Oberfläche zeigt die Auswege (ubuntu.ts vncAuthHint).
                format!("{VNC_RA2}Kein unterstütztes Anmeldeverfahren (Server bietet: {offered})")
            } else {
                format!("Kein unterstütztes Anmeldeverfahren (Server bietet: {offered})")
            });
        };
        s.write_u8(t).await.map_err(io)?;
        t
    };

    match chosen {
        1 => {}
        2 => vnc_auth(&mut s, vpw).await?,
        19 => s = vencrypt(s, l, pw, vpw).await?,
        30 => {
            let mut g = [0u8; 2];
            s.read_exact(&mut g).await.map_err(io)?;
            let len = s.read_u16().await.map_err(io)? as usize;
            let mut p = vec![0; len];
            let mut y = vec![0; len];
            s.read_exact(&mut p).await.map_err(io)?;
            s.read_exact(&mut y).await.map_err(io)?;
            let resp = auth::ard_response(&g, &p, &y, l.user, pw.unwrap_or_default())?;
            s.write_all(&resp).await.map_err(io)?;
        }
        t => return Err(format!("Anmeldeverfahren {} wird nicht unterstützt", security_name(t))),
    }

    // RFB 3.8 schickt immer ein Ergebnis, 3.3/3.7 nur nach einer echten Anmeldung.
    if (minor == 8 || chosen != 1) && s.read_u32().await.map_err(io)? != 0 {
        let reason = if minor == 8 { read_reason(&mut s).await } else { String::new() };
        return Err(format!("Anmeldung abgelehnt. {reason}").trim().to_string());
    }

    s.write_u8(1).await.map_err(io)?; // ClientInit: geteilte Sitzung
    let width = s.read_u16().await.map_err(io)?;
    let height = s.read_u16().await.map_err(io)?;
    let mut pf = [0u8; 16];
    s.read_exact(&mut pf).await.map_err(io)?;
    let len = s.read_u32().await.map_err(io)?.min(4096);
    let mut name = vec![0; len as usize];
    s.read_exact(&mut name).await.map_err(io)?;

    s.write_all(&set_pixel_format()).await.map_err(io)?;
    // Tight vor ZRLE: kompakter, und mit JPEG für Fotos und Videos viel kleiner (Server wie TigerVNC und wayvnc
    // nehmen JPEG nur, wenn der Client eine Qualitätsstufe nennt; ohne JPEG-Decoder bleibt Tight verlustfrei).
    let mut encs = vec![ENC_COPYRECT, ENC_TIGHT, ENC_ZRLE, ENC_RAW, ENC_EXT_DESKTOP_SIZE, ENC_DESKTOP_SIZE, ENC_LAST_RECT, ENC_CURSOR, ENC_EXT_CLIPBOARD, ENC_COMPRESS_2];
    if cfg!(target_os = "macos") {
        encs.push(ENC_JPEG_QUALITY_8);
    }
    s.write_all(&set_encodings(&encs))
    .await
    .map_err(io)?;
    s.flush().await.map_err(io)?;
    Ok((s, ServerInit { width, height, name: String::from_utf8_lossy(&name).into_owned() }))
}

/// JPEG für Tight: auf dem Mac über ImageIO, sonst nicht angefordert (siehe Kodierungsliste).
fn jpeg(data: &[u8]) -> Option<(usize, usize, usize, usize, Vec<u8>)> {
    #[cfg(target_os = "macos")]
    return crate::sfsymbol::decode_jpeg(data);
    #[cfg(not(target_os = "macos"))]
    {
        let _ = data;
        None
    }
}

// ---------- Client-Nachrichten ----------

fn set_pixel_format() -> Vec<u8> {
    // 32 bpp, Tiefe 24, Little Endian, True Colour, Max 255, Shift R0 G8 B16.
    vec![0, 0, 0, 0, 32, 24, 0, 1, 0, 255, 0, 255, 0, 255, 0, 8, 16, 0, 0, 0]
}

fn set_encodings(encs: &[i32]) -> Vec<u8> {
    let mut m = vec![2, 0];
    m.extend((encs.len() as u16).to_be_bytes());
    encs.iter().for_each(|e| m.extend(e.to_be_bytes()));
    m
}

fn update_request(incremental: bool, w: u16, h: u16) -> Vec<u8> {
    let mut m = vec![3, incremental as u8, 0, 0, 0, 0];
    m.extend(w.to_be_bytes());
    m.extend(h.to_be_bytes());
    m
}

pub fn pointer(mask: u8, x: u16, y: u16) -> Vec<u8> {
    let mut m = vec![5, mask];
    m.extend(x.to_be_bytes());
    m.extend(y.to_be_bytes());
    m
}

pub fn key(down: bool, keysym: u32) -> Vec<u8> {
    let mut m = vec![4, down as u8, 0, 0];
    m.extend(keysym.to_be_bytes());
    m
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Screen {
    id: u32,
    flags: u32,
}

/// Meldet der Server Bildschirme, die die Fläche deutlich nicht füllen (unter 90 % in Breite oder Höhe)?
fn fixed_monitor(fw: u16, fh: u16, bw: u16, bh: u16) -> bool {
    bw > 0 && bh > 0 && ((bw as u32) * 10 < fw as u32 * 9 || (bh as u32) * 10 < fh as u32 * 9)
}

fn set_desktop_size(w: u16, h: u16, screen: Option<Screen>) -> Vec<u8> {
    let s = screen.unwrap_or(Screen { id: 0, flags: 0 });
    let mut m = vec![251, 0];
    m.extend(w.to_be_bytes());
    m.extend(h.to_be_bytes());
    m.extend([1, 0]);
    m.extend(s.id.to_be_bytes());
    m.extend([0, 0, 0, 0]); // x, y
    m.extend(w.to_be_bytes());
    m.extend(h.to_be_bytes());
    m.extend(s.flags.to_be_bytes());
    m
}

// ---------- Sitzung ----------

#[derive(Default)]
struct Shared {
    /// Server kann SetDesktopSize (hat ExtendedDesktopSize geschickt).
    resizable: bool,
    /// Server hat eine Grössenänderung abgelehnt: nicht weiter fragen.
    refused: bool,
    screen: Option<Screen>,
    ext_clipboard: bool,
    /// Zuletzt lokal kopierter Text, für Request vom Server.
    clip_text: String,
}

/// Bilder, die die Oberfläche noch zeichnen darf: so viele gehen gleichzeitig über den Kanal, dann wartet
/// Sesam mit der nächsten Anfrage, bis eines bestätigt ist. Ohne Grenze stauen sich auf dem Mac die
/// Nachrichten in der Webansicht (jede kostet dort einen eigenen Abruf) und das Bild läuft immer weiter hinterher.
pub const FRAMES_IN_FLIGHT: usize = 2;

pub struct Session {
    out: UnboundedSender<Vec<u8>>,
    credit: Arc<tokio::sync::Semaphore>,
    shared: Arc<Mutex<Shared>>,
    reader: tokio::task::JoinHandle<()>,
}

impl Session {
    /// Startet Lese- und Schreib-Task. `emit` liefert `false`, wenn niemand mehr zuhört.
    pub fn start<S>(stream: S, init: ServerInit, emit: impl Fn(Event) -> bool + Send + Sync + 'static) -> Session
    where
        S: AsyncRead + AsyncWrite + Send + 'static,
    {
        let (rd, mut wr) = tokio::io::split(stream);
        let (out, mut rx) = unbounded_channel::<Vec<u8>>();
        tokio::spawn(async move {
            while let Some(m) = rx.recv().await {
                if wr.write_all(&m).await.is_err() {
                    break;
                }
            }
        });
        let shared = Arc::new(Mutex::new(Shared::default()));
        let credit = Arc::new(tokio::sync::Semaphore::new(FRAMES_IN_FLIGHT));
        let (o, sh, cr) = (out.clone(), shared.clone(), credit.clone());
        let reader = tokio::spawn(async move {
            emit(Event::Resize { width: init.width, height: init.height });
            let mut st = Reader { rd: BufReader::with_capacity(1 << 16, rd), fb: Framebuffer::new(init.width, init.height), zrle: Zrle::default(), tight: Tight::default(), out: o, shared: sh, credit: cr };
            let _ = st.out.send(update_request(false, init.width, init.height));
            let reason = loop {
                match st.message(&emit).await {
                    Ok(true) => {}
                    Ok(false) => return,
                    Err(e) => break e,
                }
            };
            emit(Event::Closed(reason));
        });
        Session { out, credit, shared, reader }
    }

    /// Die Oberfläche hat ein Bild gezeichnet.
    pub fn ack(&self) {
        if self.credit.available_permits() < FRAMES_IN_FLIGHT {
            self.credit.add_permits(1);
        }
    }

    pub fn send(&self, msg: Vec<u8>) {
        let _ = self.out.send(msg);
    }

    /// Entfernte Auflösung anfragen. `false`, wenn der Server das nicht kann.
    pub fn set_size(&self, w: u16, h: u16) -> bool {
        let sh = self.shared.lock().unwrap();
        if !sh.resizable || w == 0 || h == 0 {
            return false;
        }
        self.send(set_desktop_size(w, h, sh.screen));
        true
    }

    pub fn clipboard(&self, text: &str) {
        let mut sh = self.shared.lock().unwrap();
        if sh.ext_clipboard {
            sh.clip_text = text.to_string();
            self.send(clipboard::notify(true));
        } else {
            self.send(clipboard::legacy(text));
        }
    }

    pub fn close(&self) {
        self.reader.abort();
    }
}

struct Reader<R> {
    rd: BufReader<R>,
    fb: Framebuffer,
    zrle: Zrle,
    tight: Tight,
    out: UnboundedSender<Vec<u8>>,
    shared: Arc<Mutex<Shared>>,
    credit: Arc<tokio::sync::Semaphore>,
}

impl<R: AsyncRead + Unpin> Reader<R> {
    async fn bytes(&mut self, len: u32) -> Result<Vec<u8>, String> {
        if len > MAX_LEN {
            return Err("Server schickt zu grosse Daten".into());
        }
        let mut b = vec![0; len as usize];
        self.rd.read_exact(&mut b).await.map_err(|e| format!("Verbindung unterbrochen: {e}"))?;
        Ok(b)
    }

    /// Kompakte Länge (Tight): 1 bis 3 Bytes, je 7 Bit, das höchste Bit sagt «es folgt noch eins».
    async fn compact_len(&mut self) -> Result<u32, String> {
        let io = |e: std::io::Error| format!("Verbindung beendet: {e}");
        let mut len = 0u32;
        for i in 0..3 {
            let b = self.rd.read_u8().await.map_err(io)?;
            len |= u32::from(if i < 2 { b & 0x7f } else { b }) << (7 * i);
            if b & 0x80 == 0 || i == 2 {
                break;
            }
        }
        Ok(len)
    }

    async fn tight_rect(&mut self, r: Rect) -> Result<(), String> {
        let io = |e: std::io::Error| format!("Verbindung beendet: {e}");
        let ctrl = self.rd.read_u8().await.map_err(io)?;
        match self.tight.control(ctrl)? {
            TightNeed::Fill => {
                let p = self.bytes(3).await?;
                Tight::fill(&mut self.fb, r, [p[0], p[1], p[2]])
            }
            TightNeed::Jpeg => {
                let len = self.compact_len().await?;
                let data = self.bytes(len).await?;
                let (w, h, bpp, stride, px) = jpeg(&data).ok_or("Tight: JPEG nicht lesbar")?;
                Tight::pixels(&mut self.fb, r, w, h, bpp, stride, &px)
            }
            TightNeed::Basic { stream, filter } => {
                let f = if filter { self.rd.read_u8().await.map_err(io)? } else { 0 };
                let palette: Vec<[u8; 3]> = if f == 1 {
                    let n = self.rd.read_u8().await.map_err(io)? as u32 + 1;
                    self.bytes(n * 3).await?.chunks_exact(3).map(|c| [c[0], c[1], c[2]]).collect()
                } else {
                    Vec::new()
                };
                let size = Tight::data_len(r, f, palette.len());
                // Unter 12 Bytes kommt es unkomprimiert.
                let data = if size < 12 {
                    self.bytes(size as u32).await?
                } else {
                    let len = self.compact_len().await?;
                    let z = self.bytes(len).await?;
                    self.tight.inflate(stream, &z, size)?
                };
                Tight::basic(&mut self.fb, r, f, &palette, &data)
            }
        }
    }

    /// Eine Server-Nachricht verarbeiten. `Ok(false)` = Frontend weg, still beenden.
    async fn message(&mut self, emit: &impl Fn(Event) -> bool) -> Result<bool, String> {
        let io = |e: std::io::Error| format!("Verbindung beendet: {e}");
        match self.rd.read_u8().await.map_err(io)? {
            0 => {
                self.rd.read_u8().await.map_err(io)?;
                let n = self.rd.read_u16().await.map_err(io)?;
                let mut resized = false;
                let mut new_size = None;
                let mut dirty = Vec::new();
                for _ in 0..n {
                    let r = Rect {
                        x: self.rd.read_u16().await.map_err(io)?,
                        y: self.rd.read_u16().await.map_err(io)?,
                        w: self.rd.read_u16().await.map_err(io)?,
                        h: self.rd.read_u16().await.map_err(io)?,
                    };
                    match self.rd.read_i32().await.map_err(io)? {
                        ENC_RAW => {
                            let px = self.bytes(r.w as u32 * r.h as u32 * 4).await?;
                            self.fb.raw(r, &px)?;
                            add_dirty(&mut dirty, r);
                        }
                        ENC_COPYRECT => {
                            let (sx, sy) = (self.rd.read_u16().await.map_err(io)?, self.rd.read_u16().await.map_err(io)?);
                            self.fb.copy_rect(r, sx, sy)?;
                            add_dirty(&mut dirty, r);
                        }
                        ENC_ZRLE => {
                            let len = self.rd.read_u32().await.map_err(io)?;
                            let data = self.bytes(len).await?;
                            self.zrle.decode(&mut self.fb, r, &data)?;
                            add_dirty(&mut dirty, r);
                        }
                        ENC_TIGHT => {
                            self.tight_rect(r).await?;
                            add_dirty(&mut dirty, r);
                        }
                        ENC_DESKTOP_SIZE => new_size = Some((r.w, r.h)),
                        ENC_EXT_DESKTOP_SIZE => {
                            let count = self.rd.read_u8().await.map_err(io)?;
                            self.bytes(3).await?;
                            let mut first = None;
                            let (mut bw, mut bh) = (0u16, 0u16);
                            for _ in 0..count {
                                let id = self.rd.read_u32().await.map_err(io)?;
                                let (x, y, w, h) = (self.rd.read_u16().await.map_err(io)?, self.rd.read_u16().await.map_err(io)?, self.rd.read_u16().await.map_err(io)?, self.rd.read_u16().await.map_err(io)?);
                                let flags = self.rd.read_u32().await.map_err(io)?;
                                first.get_or_insert(Screen { id, flags });
                                (bw, bh) = (bw.max(x.saturating_add(w)), bh.max(y.saturating_add(h)));
                            }
                            // Fester Monitor (z. B. Raspberry Pi mit HDMI-Bildschirm): Der Server vergrössert nur die
                            // Fläche, der Desktop bleibt so gross wie der Monitor, der Rest bleibt schwarz. Dann die alte
                            // Grösse zurück und ab jetzt skalieren.
                            let fixed = r.y == 0 && fixed_monitor(r.w, r.h, bw, bh);
                            if fixed && r.x == 1 {
                                let _ = self.out.send(set_desktop_size(bw, bh, first));
                            }
                            // x = Grund (1 = Antwort auf unsere Anfrage), y = Status (0 = ok, 1–3 abgelehnt, 4 = weitergeleitet).
                            let refused = (r.x == 1 && matches!(r.y, 1..=3)) || fixed;
                            let first_refusal = {
                                let mut sh = self.shared.lock().unwrap();
                                sh.screen = first.or(sh.screen);
                                let was = !sh.refused;
                                if refused {
                                    // Nicht bei jeder Fenstergrösse neu fragen: ab jetzt skalieren.
                                    sh.resizable = false;
                                    sh.refused = true;
                                } else if !sh.refused {
                                    sh.resizable = true;
                                }
                                refused && was
                            };
                            if refused {
                                if first_refusal {
                                    let why = match r.y {
                                        _ if fixed => "fester Bildschirm",
                                        1 => "nicht erlaubt",
                                        2 => "keine Ressourcen frei",
                                        _ => "Grösse passt nicht",
                                    };
                                    if !emit(Event::Info(format!("Der Server ändert seine Auflösung nicht ({why}); die Anzeige wird skaliert."))) {
                                        return Ok(false);
                                    }
                                }
                                if !fixed {
                                    continue;
                                }
                            }
                            if r.w != self.fb.width || r.h != self.fb.height {
                                new_size = Some((r.w, r.h));
                            }
                        }
                        ENC_LAST_RECT => break,
                        ENC_CURSOR => {
                            let n = r.w as u32 * r.h as u32;
                            let px = self.bytes(n * 4).await?;
                            let mask = self.bytes((r.w as u32).div_ceil(8) * r.h as u32).await?;
                            let rgba = decode::cursor(&px, &mask, r.w, r.h);
                            if !emit(Event::Cursor { hot_x: r.x, hot_y: r.y, w: r.w, h: r.h, rgba }) {
                                return Ok(false);
                            }
                        }
                        e => return Err(format!("Server nutzt nicht angefragte Kodierung {e}")),
                    }
                    if let Some((w, h)) = new_size.take() {
                        self.fb.resize(w, h);
                        dirty.clear();
                        resized = true;
                        if !emit(Event::Resize { width: w, height: h }) {
                            return Ok(false);
                        }
                    }
                }
                if !dirty.is_empty() {
                    // Warten, bis die Oberfläche ein Bild abgearbeitet hat (höchstens 1 s, falls eine Bestätigung verloren ging).
                    if let Ok(Ok(p)) = tokio::time::timeout(std::time::Duration::from_secs(1), self.credit.acquire()).await {
                        p.forget();
                    }
                    let frame = dirty.into_iter().map(|r| (r, self.fb.crop(r))).collect();
                    if !emit(Event::Frame(frame)) {
                        return Ok(false);
                    }
                }
                let _ = self.out.send(update_request(!resized, self.fb.width, self.fb.height));
            }
            1 => {
                // SetColourMapEntries: bei True Colour unerwartet, überspringen.
                self.bytes(1).await?;
                self.rd.read_u16().await.map_err(io)?;
                let n = self.rd.read_u16().await.map_err(io)?;
                self.bytes(n as u32 * 6).await?;
            }
            2 => return Ok(emit(Event::Bell)),
            3 => {
                self.bytes(3).await?;
                let len = self.rd.read_i32().await.map_err(io)?;
                if len >= 0 {
                    let t = self.bytes(len as u32).await?;
                    return Ok(emit(Event::Clipboard(clipboard::latin1(&t))));
                }
                let data = self.bytes(len.unsigned_abs()).await?;
                if data.len() < 4 {
                    return Err("Zwischenablage: Nachricht zu kurz".into());
                }
                let flags = u32::from_be_bytes(data[..4].try_into().unwrap());
                let mut sh = self.shared.lock().unwrap();
                if flags & clipboard::CAPS != 0 {
                    sh.ext_clipboard = true;
                    let _ = self.out.send(clipboard::caps());
                } else if flags & clipboard::PROVIDE != 0 {
                    drop(sh);
                    if let Some(t) = clipboard::parse_provide(flags, &data[4..])? {
                        return Ok(emit(Event::Clipboard(t)));
                    }
                } else if flags & clipboard::REQUEST != 0 && flags & clipboard::TEXT != 0 {
                    let _ = self.out.send(clipboard::provide(&sh.clip_text));
                } else if flags & clipboard::PEEK != 0 {
                    let _ = self.out.send(clipboard::notify(!sh.clip_text.is_empty()));
                } else if flags & clipboard::NOTIFY != 0 && flags & clipboard::TEXT != 0 {
                    let _ = self.out.send(clipboard::request());
                }
            }
            t => return Err(format!("Unbekannte Server-Nachricht {t}")),
        }
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::duplex;

    fn server_init(w: u16, h: u16) -> Vec<u8> {
        let mut m = Vec::new();
        m.extend(w.to_be_bytes());
        m.extend(h.to_be_bytes());
        m.extend([0u8; 16]);
        m.extend(4u32.to_be_bytes());
        m.extend(b"Test");
        m
    }

    fn login(user: &'static str, password: Option<&'static str>) -> Login<'static> {
        Login { host: "localhost", user, password, vnc_password: None, cert: tls::Pinned::new(None) }
    }

    #[tokio::test]
    async fn vencrypt_refuses_cleartext_and_anonymous_tls() {
        let (c, mut s) = duplex(4096);
        tokio::spawn(async move {
            s.write_all(b"RFB 003.008\n").await.unwrap();
            let mut v = [0u8; 12];
            s.read_exact(&mut v).await.unwrap();
            s.write_all(&[1, 19]).await.unwrap();
            assert_eq!(s.read_u8().await.unwrap(), 19);
            s.write_all(&[0, 2]).await.unwrap();
            let mut v = [0u8; 2];
            s.read_exact(&mut v).await.unwrap();
            assert_eq!(v, [0, 2]);
            s.write_all(&[0, 2]).await.unwrap(); // ok, 2 Untertypen
            s.write_u32(256).await.unwrap(); // Plain (Klartext)
            s.write_u32(259).await.unwrap(); // TLSPlain (anonym)
        });
        let e = handshake(c, &login("mike", Some("pw"))).await.err().unwrap();
        assert!(e.contains("X509"), "{e}");
    }

    #[test]
    fn cert_pinning_explains_unknown_and_changed() {
        use rustls::client::danger::ServerCertVerifier;
        use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
        let der = CertificateDer::from(vec![1u8, 2, 3]);
        let fp = tls::fingerprint(&der);
        let name = ServerName::try_from("host").unwrap();
        let check = |p: &tls::Pinned| p.verify_server_cert(&der, &[], &name, &[], UnixTime::now()).is_ok();
        let err = || std::io::Error::other("x");

        let unknown = tls::Pinned::new(None);
        assert!(!check(&unknown));
        assert_eq!(unknown.explain(err()), format!("{}{fp}", tls::CERT_UNKNOWN));

        let changed = tls::Pinned::new(Some("SHA256:anders".into()));
        assert!(!check(&changed));
        assert!(changed.explain(err()).starts_with(tls::CERT_CHANGED));

        assert!(check(&tls::Pinned::new(Some(fp))));
    }

    #[tokio::test]
    async fn handshake_38_with_vnc_password() {
        let (c, mut s) = duplex(4096);
        let server = tokio::spawn(async move {
            s.write_all(b"RFB 003.008\n").await.unwrap();
            let mut v = [0u8; 12];
            s.read_exact(&mut v).await.unwrap();
            assert_eq!(&v, b"RFB 003.008\n");
            s.write_all(&[2, 16, 2]).await.unwrap(); // Tight (nicht unterstützt), VNC
            assert_eq!(s.read_u8().await.unwrap(), 2);
            let ch = [5u8; 16];
            s.write_all(&ch).await.unwrap();
            let mut resp = [0u8; 16];
            s.read_exact(&mut resp).await.unwrap();
            assert_eq!(resp, auth::vnc_response(b"vncpw", &ch));
            s.write_u32(0).await.unwrap();
            assert_eq!(s.read_u8().await.unwrap(), 1);
            s.write_all(&server_init(800, 600)).await.unwrap();
            let mut pf = [0u8; 20];
            s.read_exact(&mut pf).await.unwrap();
            assert_eq!(pf[0], 0);
            assert_eq!(s.read_u8().await.unwrap(), 2); // SetEncodings
        });
        let l = Login { vnc_password: Some("vncpw"), ..login("", Some("konto")) };
        let (_, init) = handshake(c, &l).await.unwrap();
        assert_eq!(init, ServerInit { width: 800, height: 600, name: "Test".into() });
        server.await.unwrap();
    }

    #[tokio::test]
    async fn handshake_macos_version_and_errors() {
        let (c, mut s) = duplex(4096);
        tokio::spawn(async move {
            s.write_all(b"RFB 003.889\n").await.unwrap();
            let mut v = [0u8; 12];
            s.read_exact(&mut v).await.unwrap();
            assert_eq!(&v, b"RFB 003.008\n");
            s.write_all(&[2, 30, 2]).await.unwrap();
        });
        let e = handshake(c, &login("mike", None)).await.err().unwrap();
        assert!(e.contains("Passwort"), "{e}");
    }

    #[tokio::test]
    async fn handshake_33_without_auth() {
        let (c, mut s) = duplex(4096);
        tokio::spawn(async move {
            s.write_all(b"RFB 003.003\n").await.unwrap();
            let mut v = [0u8; 12];
            s.read_exact(&mut v).await.unwrap();
            assert_eq!(&v, b"RFB 003.003\n");
            s.write_u32(1).await.unwrap();
            assert_eq!(s.read_u8().await.unwrap(), 1);
            s.write_all(&server_init(2, 2)).await.unwrap();
            let mut rest = vec![0u8; 20 + 4 + 8 * 4];
            s.read_exact(&mut rest).await.unwrap();
        });
        assert_eq!(handshake(c, &login("", None)).await.unwrap().1.width, 2);
    }

    #[tokio::test]
    async fn session_updates_resize_and_clipboard() {
        let (c, mut s) = duplex(1 << 16);
        let (tx, mut rx) = unbounded_channel();
        let session = Session::start(c, ServerInit { width: 2, height: 1, name: String::new() }, move |e| tx.send(e).is_ok());
        assert_eq!(rx.recv().await, Some(Event::Resize { width: 2, height: 1 }));

        // Erste Anfrage ist nicht inkrementell.
        let mut req = [0u8; 10];
        s.read_exact(&mut req).await.unwrap();
        assert_eq!(req, [3, 0, 0, 0, 0, 0, 0, 2, 0, 1]);

        // ExtendedDesktopSize (Bildschirm 7) + Raw-Rechteck.
        let mut m = vec![0, 0, 0, 2];
        m.extend([0, 0, 0, 0, 0, 2, 0, 1]);
        m.extend(ENC_EXT_DESKTOP_SIZE.to_be_bytes());
        m.extend([1, 0, 0, 0]);
        m.extend(7u32.to_be_bytes());
        m.extend([0, 0, 0, 0, 0, 2, 0, 1]);
        m.extend(0u32.to_be_bytes());
        m.extend([0, 0, 0, 0, 0, 1, 0, 1]);
        m.extend(ENC_RAW.to_be_bytes());
        m.extend([1, 2, 3, 0]);
        s.write_all(&m).await.unwrap();
        let ev = rx.recv().await.unwrap();
        assert_eq!(ev, Event::Frame(vec![(Rect { x: 0, y: 0, w: 1, h: 1 }, vec![1, 2, 3, 255])]));
        s.read_exact(&mut req).await.unwrap();
        assert_eq!(req[1], 1, "danach inkrementell");

        assert!(session.set_size(1920, 1080));
        let mut sds = [0u8; 24];
        s.read_exact(&mut sds).await.unwrap();
        assert_eq!(&sds[..6], &[251, 0, 0x07, 0x80, 0x04, 0x38]);
        assert_eq!(&sds[8..12], &7u32.to_be_bytes());

        // Server lehnt ab (Status 1): einmal ruhig melden, danach nicht mehr fragen.
        let mut no = vec![0, 0, 0, 1, 0, 1, 0, 1, 0x07, 0x80, 0x04, 0x38];
        no.extend(ENC_EXT_DESKTOP_SIZE.to_be_bytes());
        no.extend([1, 0, 0, 0]);
        no.extend(7u32.to_be_bytes());
        no.extend([0, 0, 0, 0, 0, 2, 0, 1]);
        no.extend(0u32.to_be_bytes());
        s.write_all(&no).await.unwrap();
        assert!(matches!(rx.recv().await, Some(Event::Info(t)) if t.contains("nicht erlaubt") && t.contains("skaliert")));
        s.read_exact(&mut req).await.unwrap();
        assert!(!session.set_size(1280, 720), "nach Ablehnung nicht mehr fragen");

        // Fester Monitor: Fläche 1920×1080, Bildschirm nur 1280×720 → alte Grösse zurück, skalieren.
        assert!(fixed_monitor(1920, 1080, 1280, 720) && !fixed_monitor(1920, 1080, 1920, 1080) && !fixed_monitor(1920, 1080, 0, 0));
        assert!(!fixed_monitor(2560, 1440, 2560, 1400) && fixed_monitor(2752, 1800, 1920, 1080));

        // Alte ServerCutText-Nachricht (Latin-1).
        s.write_all(&[3, 0, 0, 0, 0, 0, 0, 2, 0xE4, b'!']).await.unwrap();
        assert_eq!(rx.recv().await, Some(Event::Clipboard("ä!".into())));

        // Server meldet Extended Clipboard; danach geht Text als Notify raus.
        let mut caps = vec![3, 0, 0, 0];
        caps.extend((-8i32).to_be_bytes());
        caps.extend((clipboard::CAPS | clipboard::TEXT).to_be_bytes());
        caps.extend(1024u32.to_be_bytes());
        s.write_all(&caps).await.unwrap();
        let mut ours = vec![0u8; clipboard::caps().len()];
        s.read_exact(&mut ours).await.unwrap();
        assert_eq!(ours, clipboard::caps());
        session.clipboard("Grüezi");
        let mut n = vec![0u8; clipboard::notify(true).len()];
        s.read_exact(&mut n).await.unwrap();
        assert_eq!(n, clipboard::notify(true));

        // Fehlerhaftes Rechteck beendet die Sitzung mit Meldung statt Absturz.
        let mut bad = vec![0, 0, 0, 1, 0, 5, 0, 5, 0, 1, 0, 1];
        bad.extend(ENC_RAW.to_be_bytes());
        bad.extend([0u8; 4]);
        s.write_all(&bad).await.unwrap();
        assert!(matches!(rx.recv().await, Some(Event::Closed(e)) if e.contains("ausserhalb")));
        session.close();
    }
}
