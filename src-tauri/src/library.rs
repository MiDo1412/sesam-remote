//! Bibliothek: Verbindungsliste ohne Geheimnisse als JSON im App-Datenordner.

use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Connection {
    pub id: String,
    pub name: String,
    pub group: String,
    pub address: String,
    /// `None` = Desktop (VNC) ausgeschaltet.
    pub vnc_port: Option<u16>,
    /// `None` = Shell/Dateien (SSH) ausgeschaltet.
    pub ssh_port: Option<u16>,
    /// `None` = Desktop über RDP ausgeschaltet.
    pub rdp_port: Option<u16>,
    /// Windows-Domäne für RDP (leer = keine).
    pub rdp_domain: String,
    /// Eigener RDP-Benutzer (leer = Benutzer der Verbindung), z. B. GNOME-Freigabe.
    pub rdp_user: String,
    /// Was RDP öffnet: "" = Windows/anderer Server, "login" = GNOME-Anmeldebildschirm, "session" = laufende Sitzung übernehmen.
    pub rdp_target: String,
    /// Bei "session": wessen Sitzung (nur für Anzeige und Meldungen).
    pub rdp_session_user: String,
    /// Bei VNC *und* RDP: womit «Desktop» verbindet ("vnc" | "rdp").
    pub desktop_protocol: String,
    pub user: String,
    /// "desktop" | "shell" | "files"
    pub default_view: String,
    /// "landscape" | "portrait"
    pub orientation: String,
    pub shared_clipboard: bool,
    /// Ton des entfernten Rechners abspielen (RDP).
    pub play_audio: bool,
    /// Lokaler Ordner, der per RDP als Laufwerk erscheint (leer = aus).
    pub shared_folder: String,
    /// TLS-Zertifikat nicht prüfen (RDP/VNC). Unsicher, nur bewusst einschalten.
    pub skip_cert_check: bool,
    /// Betriebssystem für das Symbol: "ubuntu" | "linux" | "macos" | "windows" | "raspberry" | "".
    pub os: String,
    /// Eigenes Bild statt des letzten Desktop-Bildes.
    pub custom_thumb: bool,
    /// Gespeicherte Zugangsdaten statt eigener (Benutzername, Passwort, SSH-Schlüssel).
    pub credential_id: Option<String>,
    /// Gespeicherte Zugangsdaten nur für RDP, VNC (Passwort) bzw. das RD-Gateway.
    pub rdp_credential_id: Option<String>,
    pub vnc_credential_id: Option<String>,
    pub gateway_credential_id: Option<String>,
    /// RDP: nur Kerberos, kein Rückfall auf NTLM.
    pub rdp_no_ntlm: bool,
    /// RDP: KDC (`host[:port]`, `tcp://`, `udp://`) oder KDC-Proxy (`https://…/KdcProxy`); leer = per DNS.
    pub rdp_kdc: String,
    /// RDP: RD-Gateway `host[:port]` (leer = direkt).
    pub rdp_gateway: String,
    /// Eigener Gateway-Benutzer (leer = gleiche Zugangsdaten wie die Verbindung).
    pub rdp_gateway_user: String,
    pub fit_to_window: bool,
    /// Nur Merker für die Anzeige; das Geheimnis selbst liegt im Tresor.
    pub has_password: bool,
    pub has_vnc_password: bool,
    pub has_rdp_password: bool,
    pub has_gateway_password: bool,
    pub has_key: bool,
    /// Dateiübertragung: "auto" (SFTP, sonst SCP) | "sftp" | "scp".
    pub file_transfer: String,
    /// SSH: veraltete Algorithmen erlauben (SHA-1, CBC, 3DES, ssh-rsa).
    pub ssh_legacy: bool,
    /// SSH-Keepalive in Sekunden, 0 = aus.
    pub ssh_keepalive: u32,
    pub ssh_compression: bool,
    /// Portweiterleitungen der Shell, je eine Regel wie `L8080 localhost:80`, `R9000 localhost:3000`, `D1080`.
    pub forwards: Vec<String>,
    /// Sprungserver (ProxyJump): ID einer anderen Verbindung der Bibliothek, leer = direkt.
    pub jump_host: String,
    /// Zuerst mit dem SSH-Agenten anmelden (Pageant oder Windows-OpenSSH-Agent).
    pub ssh_agent: bool,
    /// Agenten an den Server weiterreichen (nur mit `ssh_agent`).
    pub agent_forward: bool,
    pub last_used: Option<String>,
}

impl Default for Connection {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            group: String::new(),
            address: String::new(),
            vnc_port: Some(5900),
            ssh_port: Some(22),
            rdp_port: None,
            rdp_domain: String::new(),
            rdp_user: String::new(),
            rdp_target: String::new(),
            rdp_session_user: String::new(),
            desktop_protocol: "vnc".into(),
            user: String::new(),
            default_view: "desktop".into(),
            orientation: "landscape".into(),
            shared_clipboard: true,
            play_audio: true,
            shared_folder: String::new(),
            skip_cert_check: false,
            os: String::new(),
            custom_thumb: false,
            credential_id: None,
            rdp_credential_id: None,
            vnc_credential_id: None,
            gateway_credential_id: None,
            rdp_no_ntlm: false,
            rdp_kdc: String::new(),
            rdp_gateway: String::new(),
            rdp_gateway_user: String::new(),
            fit_to_window: true,
            has_password: false,
            has_vnc_password: false,
            has_rdp_password: false,
            has_gateway_password: false,
            has_key: false,
            file_transfer: "auto".into(),
            ssh_legacy: false,
            ssh_keepalive: 0,
            ssh_compression: false,
            forwards: Vec::new(),
            jump_host: String::new(),
            ssh_agent: false,
            agent_forward: false,
            last_used: None,
        }
    }
}

impl Connection {
    /// Alle zugewiesenen gespeicherten Zugangsdaten (Anmeldung, RDP, VNC, Gateway).
    pub fn cred_ids(&self) -> [Option<&str>; 4] {
        [&self.credential_id, &self.rdp_credential_id, &self.vnc_credential_id, &self.gateway_credential_id].map(|x| x.as_deref())
    }

    pub fn validate(&self) -> Result<(), String> {
        if !["", "login", "session"].contains(&self.rdp_target.as_str()) {
            return Err("Ungültige Auswahl für RDP".into());
        }
        if self.id.trim().is_empty() || self.id.len() > 64 || !self.id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
            return Err("Ungültige ID".into());
        }
        if self.name.trim().is_empty() {
            return Err("Name fehlt".into());
        }
        if self.address.trim().is_empty() || self.address.chars().any(char::is_whitespace) {
            return Err("Adresse fehlt oder enthält Leerzeichen".into());
        }
        if self.vnc_port == Some(0) || self.ssh_port == Some(0) || self.rdp_port == Some(0) {
            return Err("Port 0 ist ungültig".into());
        }
        if !["desktop", "shell", "files"].contains(&self.default_view.as_str()) {
            return Err("Unbekannte Standardansicht".into());
        }
        if !["vnc", "rdp"].contains(&self.desktop_protocol.as_str()) {
            return Err("Unbekanntes Desktop-Protokoll".into());
        }
        if !["auto", "sftp", "scp"].contains(&self.file_transfer.as_str()) {
            return Err("Unbekannte Dateiübertragung".into());
        }
        if self.forwards.len() > 32 {
            return Err("Höchstens 32 Weiterleitungen".into());
        }
        for f in &self.forwards {
            crate::forward::parse(f)?;
        }
        if self.jump_host == self.id || !self.jump_host.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
            return Err("Ungültiger Sprungserver".into());
        }
        if !["landscape", "portrait"].contains(&self.orientation.as_str()) {
            return Err("Unbekannte Ausrichtung".into());
        }
        crate::kdc::kdc_url(&self.rdp_kdc)?;
        if self.rdp_gateway.contains(char::is_whitespace) || self.rdp_gateway.contains('/') {
            return Err("RD-Gateway: nur host[:port]".into());
        }
        Ok(())
    }
}

pub fn load(path: &Path) -> Result<Vec<Connection>, String> {
    match std::fs::read_to_string(path) {
        Ok(s) => serde_json::from_str(&s).map_err(|e| format!("Bibliothek ist beschädigt: {e}")),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(e) => Err(e.to_string()),
    }
}

/// Schreibt zuerst eine Temp-Datei und benennt sie dann um, damit ein Absturz die Liste nicht zerstört.
pub fn save(path: &Path, items: &[Connection]) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let tmp = path.with_extension("json.tmp");
    let json = serde_json::to_string_pretty(items).map_err(|e| e.to_string())?;
    std::fs::write(&tmp, json).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, path).map_err(|e| e.to_string())
}

pub fn upsert(items: &mut Vec<Connection>, c: Connection) {
    match items.iter_mut().find(|x| x.id == c.id) {
        Some(x) => *x = c,
        None => items.push(c),
    }
}

pub fn remove(items: &mut Vec<Connection>, id: &str) -> bool {
    let n = items.len();
    items.retain(|x| x.id != id);
    items.len() != n
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_upsert_remove() {
        let path = std::env::temp_dir().join(format!("sesam-test-{}/library.json", std::process::id()));
        assert!(load(&path).unwrap().is_empty());

        let mut items = Vec::new();
        let mut c = Connection { id: "a-1".into(), name: "Homelab".into(), address: "192.168.1.20".into(), ..Default::default() };
        c.validate().unwrap();
        upsert(&mut items, c.clone());
        c.name = "Homelab Ubuntu".into();
        upsert(&mut items, c.clone());
        assert_eq!(items.len(), 1);
        save(&path, &items).unwrap();
        assert_eq!(load(&path).unwrap(), vec![c]);

        assert!(remove(&mut items, "a-1"));
        assert!(!remove(&mut items, "a-1"));
        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[test]
    fn rejects_bad_input() {
        let ok = Connection { id: "x".into(), name: "n".into(), address: "h".into(), ..Default::default() };
        assert!(ok.validate().is_ok());
        assert!(Connection { id: "../x".into(), ..ok.clone() }.validate().is_err());
        assert!(Connection { address: "a b".into(), ..ok.clone() }.validate().is_err());
        assert!(Connection { ssh_port: Some(0), ..ok.clone() }.validate().is_err());
        assert!(Connection { rdp_port: Some(0), ..ok.clone() }.validate().is_err());
        assert!(Connection { desktop_protocol: "x".into(), ..ok.clone() }.validate().is_err());
        assert!(Connection { rdp_kdc: "ftp://dc".into(), ..ok.clone() }.validate().is_err());
        assert!(Connection { rdp_gateway: "https://gw/x".into(), ..ok.clone() }.validate().is_err());
        assert!(Connection { rdp_kdc: "https://gw.contoso.ch/KdcProxy".into(), rdp_gateway: "gw.contoso.ch:443".into(), ..ok.clone() }.validate().is_ok());
        assert!(Connection { file_transfer: "ftp".into(), ..ok.clone() }.validate().is_err());
        assert!(Connection { forwards: vec!["L8080 localhost:80".into(), "D1080".into()], ..ok.clone() }.validate().is_ok());
        assert!(Connection { forwards: vec!["L8080".into()], ..ok.clone() }.validate().is_err());
        assert!(Connection { jump_host: "x".into(), ..ok.clone() }.validate().is_err());
        assert!(Connection { jump_host: "bastion-1".into(), ..ok.clone() }.validate().is_ok());
        assert!(Connection { default_view: "x".into(), ..ok }.validate().is_err());
    }

    #[test]
    fn old_json_without_new_fields_loads() {
        let c: Connection = serde_json::from_str(r#"{"id":"a","name":"n","address":"h"}"#).unwrap();
        assert_eq!(c.ssh_port, Some(22));
        assert_eq!((c.rdp_port, c.desktop_protocol.as_str()), (None, "vnc"));
        assert_eq!(c.file_transfer, "auto");
    }
}
