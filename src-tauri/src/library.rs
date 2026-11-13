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
    /// Touch-Bedienung des Desktops: "touchpad" (Zeiger schieben) | "direct" (direkt tippen); leer = Touchpad.
    pub touch_mode: String,
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
    /// `None` = Dateien über FTP ausgeschaltet.
    pub ftp_port: Option<u16>,
    /// FTP-Verschlüsselung: "explicit" (FTPS mit AUTH TLS) | "implicit" (FTPS ab Verbindungsbeginn) | "none".
    pub ftp_security: String,
    /// FTP im aktiven Modus (Server verbindet zurück); sonst passiv.
    pub ftp_active: bool,
    /// Bei SSH *und* FTP: womit «Dateien» verbindet ("ssh" | "ftp").
    pub files_via: String,
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
    /// MAC-Adresse für Wake-on-LAN (selbst ermittelt, wenn der Rechner einmal erreichbar war, oder eingetragen).
    pub mac: String,
    /// Desktop (RDP/VNC) durch einen SSH-Tunnel: der Dienst muss nicht offen im Netz stehen.
    pub ssh_tunnel: bool,
    /// Shell auf dem Server halten: "" = aus, "tmux" = in einer tmux-Sitzung (übersteht Unterbrüche).
    pub persist_shell: String,
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
            touch_mode: String::new(),
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
            ftp_port: None,
            ftp_security: "explicit".into(),
            ftp_active: false,
            files_via: "ssh".into(),
            ssh_legacy: false,
            ssh_keepalive: 0,
            ssh_compression: false,
            forwards: Vec::new(),
            jump_host: String::new(),
            ssh_agent: false,
            agent_forward: false,
            last_used: None,
            mac: String::new(),
            ssh_tunnel: false,
            persist_shell: String::new(),
        }
    }
}

impl Connection {
    /// Alle zugewiesenen gespeicherten Zugangsdaten (Anmeldung, RDP, VNC, Gateway).
    pub fn cred_ids(&self) -> [Option<&str>; 4] {
        [&self.credential_id, &self.rdp_credential_id, &self.vnc_credential_id, &self.gateway_credential_id].map(|x| x.as_deref())
    }

    /// «Dateien» über FTP: FTP an und entweder kein SSH oder bei beiden FTP gewählt.
    pub fn files_over_ftp(&self) -> bool {
        self.ftp_port.is_some() && (self.ssh_port.is_none() || self.files_via == "ftp")
    }

    pub fn validate(&self) -> Result<(), String> {
        if !["", "tmux"].contains(&self.persist_shell.as_str()) {
            return Err("Ungültige Auswahl für «Shell halten»".into());
        }
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
        if self.vnc_port == Some(0) || self.ssh_port == Some(0) || self.rdp_port == Some(0) || self.ftp_port == Some(0) {
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
        crate::ftp::Security::parse(&self.ftp_security)?;
        if !["ssh", "ftp"].contains(&self.files_via.as_str()) {
            return Err("Unbekannter Weg für Dateien".into());
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
pub fn save<T: Serialize>(path: &Path, items: &[T]) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let tmp = path.with_extension("json.tmp");
    let json = serde_json::to_string_pretty(items).map_err(|e| e.to_string())?;
    std::fs::write(&tmp, json).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, path).map_err(|e| e.to_string())?;
    crate::jumps::changed(path);
    Ok(())
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

/// Gruppe der Seitenleiste: Reihenfolge und Farbe. Liegt in `groups.json` neben der Bibliothek;
/// welche Verbindung in welcher Gruppe ist, steht weiter im Feld `group` der Verbindung.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Group {
    pub name: String,
    /// "" = keine, sonst eine der Vorgaben in `GROUP_COLORS`.
    pub color: String,
}

pub const GROUP_COLORS: [&str; 8] = ["", "red", "orange", "yellow", "green", "blue", "purple", "gray"];

pub fn validate_group_name(name: &str) -> Result<(), String> {
    if name.trim().is_empty() || name.trim() != name {
        return Err("Gruppenname fehlt oder beginnt/endet mit Leerzeichen".into());
    }
    if name.chars().count() > 100 || name.chars().any(char::is_control) {
        return Err("Gruppenname zu lang oder mit Steuerzeichen".into());
    }
    Ok(())
}

pub fn validate_groups(groups: &[Group]) -> Result<(), String> {
    if groups.len() > 1000 {
        return Err("Zu viele Gruppen".into());
    }
    for (i, g) in groups.iter().enumerate() {
        validate_group_name(&g.name)?;
        if !GROUP_COLORS.contains(&g.color.as_str()) {
            return Err("Unbekannte Gruppenfarbe".into());
        }
        if groups[..i].iter().any(|x| x.name == g.name) {
            return Err(format!("Gruppe «{}» doppelt", g.name));
        }
    }
    Ok(())
}

pub fn load_groups(path: &Path) -> Result<Vec<Group>, String> {
    match std::fs::read_to_string(path) {
        Ok(s) => serde_json::from_str(&s).map_err(|e| format!("Gruppenliste ist beschädigt: {e}")),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(e) => Err(e.to_string()),
    }
}

/// Gespeicherte Gruppen in ihrer Reihenfolge, dahinter alphabetisch die, die nur in Verbindungen vorkommen.
pub fn merged_groups(mut stored: Vec<Group>, items: &[Connection]) -> Vec<Group> {
    let mut extra: Vec<&str> = items.iter().map(|c| c.group.as_str()).filter(|g| !g.is_empty() && !stored.iter().any(|s| s.name == *g)).collect();
    extra.sort_unstable();
    extra.dedup();
    extra.sort_by_key(|g| g.to_lowercase());
    stored.extend(extra.into_iter().map(|name| Group { name: name.into(), color: String::new() }));
    stored
}

/// Umbenennen; gibt es den neuen Namen schon, werden die Gruppen zusammengeführt (Farbe/Platz des Ziels).
pub fn rename_group(items: &mut [Connection], groups: &mut Vec<Group>, from: &str, to: &str) -> Result<(), String> {
    validate_group_name(to)?;
    if from == to {
        return Ok(());
    }
    for c in items.iter_mut().filter(|c| c.group == from) {
        c.group = to.into();
    }
    if groups.iter().any(|g| g.name == to) {
        groups.retain(|g| g.name != from);
    } else if let Some(g) = groups.iter_mut().find(|g| g.name == from) {
        g.name = to.into();
    }
    Ok(())
}

/// Gruppe entfernen; ihre Verbindungen kommen nach «Ohne Gruppe».
pub fn delete_group(items: &mut [Connection], groups: &mut Vec<Group>, name: &str) {
    for c in items.iter_mut().filter(|c| c.group == name) {
        c.group.clear();
    }
    groups.retain(|g| g.name != name);
}

/// Zwei Einträge desselben Geräts zusammenführen: `target` übernimmt die Dienste von `source`, die ihm fehlen.
/// Haben beide denselben Dienst (z. B. RDP auf verschiedenen Ports), geht es nicht: das wären zwei Sitzungen,
/// keine zwei Türen desselben Geräts. Gibt die übernommenen Dienste zurück.
pub fn merge(target: &mut Connection, source: &Connection) -> Result<Vec<&'static str>, String> {
    if target.address.trim().to_lowercase() != source.address.trim().to_lowercase() {
        return Err("Nur Einträge mit derselben Adresse lassen sich zusammenführen".into());
    }
    let both: Vec<&str> = [("RDP", target.rdp_port.is_some() && source.rdp_port.is_some()), ("VNC", target.vnc_port.is_some() && source.vnc_port.is_some()), ("SSH", target.ssh_port.is_some() && source.ssh_port.is_some()), ("FTP", target.ftp_port.is_some() && source.ftp_port.is_some())]
        .into_iter()
        .filter(|(_, b)| *b)
        .map(|(n, _)| n)
        .collect();
    let mut taken = Vec::new();
    if target.rdp_port.is_none() && source.rdp_port.is_some() {
        target.rdp_port = source.rdp_port;
        target.rdp_domain.clone_from(&source.rdp_domain);
        target.rdp_user.clone_from(&source.rdp_user);
        target.rdp_target.clone_from(&source.rdp_target);
        target.rdp_session_user.clone_from(&source.rdp_session_user);
        target.rdp_credential_id.clone_from(&source.rdp_credential_id);
        target.rdp_gateway.clone_from(&source.rdp_gateway);
        target.rdp_gateway_user.clone_from(&source.rdp_gateway_user);
        target.gateway_credential_id.clone_from(&source.gateway_credential_id);
        taken.push("RDP");
    }
    if target.vnc_port.is_none() && source.vnc_port.is_some() {
        target.vnc_port = source.vnc_port;
        target.vnc_credential_id.clone_from(&source.vnc_credential_id);
        taken.push("VNC");
    }
    if target.ssh_port.is_none() && source.ssh_port.is_some() {
        target.ssh_port = source.ssh_port;
        target.forwards.clone_from(&source.forwards);
        target.jump_host.clone_from(&source.jump_host);
        taken.push("SSH");
    }
    if target.ftp_port.is_none() && source.ftp_port.is_some() {
        target.ftp_port = source.ftp_port;
        target.ftp_security.clone_from(&source.ftp_security);
        target.ftp_active = source.ftp_active;
        taken.push("FTP");
    }
    if taken.is_empty() {
        return Err(if both.is_empty() { "Nichts zu übernehmen".into() } else { format!("Beide Einträge haben {}; das sind zwei Sitzungen, nicht ein Gerät", both.join(" und ")) });
    }
    if target.user.is_empty() {
        target.user.clone_from(&source.user);
    }
    if target.credential_id.is_none() {
        target.credential_id.clone_from(&source.credential_id);
    }
    if target.os.is_empty() {
        target.os.clone_from(&source.os);
    }
    if target.mac.is_empty() {
        target.mac.clone_from(&source.mac);
    }
    Ok(taken)
}

#[cfg(test)]
mod tests {
    #[test]
    fn zusammenfuehren_uebernimmt_fehlende_dienste() {
        use super::{Connection, merge};
        let mut shell = Connection { address: "Pi.local".into(), ssh_port: Some(22), vnc_port: None, user: "pi".into(), ..Default::default() };
        let desk = Connection { address: "pi.local".into(), ssh_port: None, vnc_port: Some(5900), vnc_credential_id: Some("c1".into()), os: "raspberry".into(), ..Default::default() };
        assert_eq!(merge(&mut shell, &desk).unwrap(), vec!["VNC"]);
        assert_eq!((shell.vnc_port, shell.vnc_credential_id.as_deref(), shell.os.as_str(), shell.user.as_str()), (Some(5900), Some("c1"), "raspberry", "pi"));
        // Zwei RDP-Einträge (z. B. Anmeldebildschirm und Sitzung) bleiben getrennt.
        let mut a = Connection { address: "pc".into(), rdp_port: Some(3389), vnc_port: None, ssh_port: None, ..Default::default() };
        let b = Connection { address: "pc".into(), rdp_port: Some(3390), vnc_port: None, ssh_port: None, ..Default::default() };
        assert!(merge(&mut a, &b).unwrap_err().contains("RDP"));
        let mut c = Connection { address: "x".into(), ..Default::default() };
        assert!(merge(&mut c, &Connection { address: "y".into(), ..Default::default() }).is_err());
    }

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
        assert!(Connection { ftp_port: Some(0), ..ok.clone() }.validate().is_err());
        assert!(Connection { ftp_security: "tls".into(), ..ok.clone() }.validate().is_err());
        assert!(Connection { files_via: "x".into(), ..ok.clone() }.validate().is_err());
        let ftp = Connection { ftp_port: Some(21), ..ok.clone() };
        assert!(ftp.validate().is_ok() && !ftp.files_over_ftp(), "SSH geht vor, solange nicht FTP gewählt ist");
        assert!(Connection { ssh_port: None, ..ftp.clone() }.files_over_ftp());
        assert!(Connection { files_via: "ftp".into(), ..ftp }.files_over_ftp());
        assert!(Connection { forwards: vec!["L8080 localhost:80".into(), "D1080".into()], ..ok.clone() }.validate().is_ok());
        assert!(Connection { forwards: vec!["L8080".into()], ..ok.clone() }.validate().is_err());
        assert!(Connection { jump_host: "x".into(), ..ok.clone() }.validate().is_err());
        assert!(Connection { jump_host: "bastion-1".into(), ..ok.clone() }.validate().is_ok());
        assert!(Connection { default_view: "x".into(), ..ok }.validate().is_err());
    }

    fn conn(id: &str, group: &str) -> Connection {
        Connection { id: id.into(), name: id.into(), address: "h".into(), group: group.into(), ..Default::default() }
    }
    fn g(name: &str, color: &str) -> Group {
        Group { name: name.into(), color: color.into() }
    }

    #[test]
    fn groups_order_roundtrip_and_merge() {
        let path = std::env::temp_dir().join(format!("sesam-groups-{}/groups.json", std::process::id()));
        assert!(load_groups(&path).unwrap().is_empty());
        let stored = vec![g("Kunden", "blue"), g("Leer", "")];
        validate_groups(&stored).unwrap();
        save(&path, &stored).unwrap();
        assert_eq!(load_groups(&path).unwrap(), stored);
        // Gruppen nur aus Verbindungen hinten anhängen, alphabetisch, ohne Doppel.
        let items = [conn("a", "homelab"), conn("b", "Büro"), conn("c", "Kunden"), conn("d", "homelab"), conn("e", "")];
        let names: Vec<_> = merged_groups(load_groups(&path).unwrap(), &items).into_iter().map(|x| x.name).collect();
        assert_eq!(names, ["Kunden", "Leer", "Büro", "homelab"]);
        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[test]
    fn rename_and_delete_group() {
        let mut items = vec![conn("a", "Alt"), conn("b", "Alt"), conn("c", "Neu"), conn("d", "")];
        let mut groups = vec![g("Alt", "red"), g("Neu", "green")];
        rename_group(&mut items, &mut groups, "Alt", "Büro").unwrap();
        assert_eq!(items.iter().map(|c| c.group.as_str()).collect::<Vec<_>>(), ["Büro", "Büro", "Neu", ""]);
        assert_eq!(groups, [g("Büro", "red"), g("Neu", "green")]);
        // Auf bestehenden Namen: zusammenführen, Ziel behält Farbe und Platz.
        rename_group(&mut items, &mut groups, "Büro", "Neu").unwrap();
        assert!(items[..3].iter().all(|c| c.group == "Neu"));
        assert_eq!(groups, [g("Neu", "green")]);
        assert!(rename_group(&mut items, &mut groups, "Neu", " ").is_err());
        assert!(rename_group(&mut items, &mut groups, "Neu", " x").is_err());

        delete_group(&mut items, &mut groups, "Neu");
        assert!(items.iter().all(|c| c.group.is_empty()));
        assert!(groups.is_empty());
    }

    #[test]
    fn rejects_bad_groups() {
        assert!(validate_groups(&[g("A", "pink")]).is_err());
        assert!(validate_groups(&[g("A", ""), g("A", "red")]).is_err());
        assert!(validate_groups(&[g("", "")]).is_err());
        assert!(validate_groups(&[g("a\nb", "")]).is_err());
        assert!(validate_groups(&[g(&"x".repeat(101), "")]).is_err());
        assert!(validate_groups(&[g("A", "gray"), g("a", "")]).is_ok());
    }

    #[test]
    fn old_json_without_new_fields_loads() {
        let c: Connection = serde_json::from_str(r#"{"id":"a","name":"n","address":"h"}"#).unwrap();
        assert_eq!(c.ssh_port, Some(22));
        assert_eq!((c.rdp_port, c.desktop_protocol.as_str()), (None, "vnc"));
        assert_eq!(c.file_transfer, "auto");
        assert_eq!((c.ftp_port, c.ftp_security.as_str(), c.files_via.as_str()), (None, "explicit", "ssh"));
    }
}
