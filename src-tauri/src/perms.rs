//! «Berechtigungen einrichten …»: Freigaben des Betriebssystems, die Sesam braucht, mit Status, Anfrage und
//! dem passenden Ort in den Systemeinstellungen. Namen wie in macOS 27 (Systemeinstellungen › Datenschutz & Sicherheit).

#[derive(serde::Serialize, Clone)]
pub struct Perm {
    pub id: &'static str,
    pub label: &'static str,
    pub purpose: &'static str,
    /// "ok", "missing" oder "unknown" (macOS verrät den Stand nicht, z. B. Lokales Netzwerk).
    pub state: &'static str,
}

#[cfg(target_os = "macos")]
mod imp {
    use super::Perm;
    use objc2_core_graphics::{CGPreflightListenEventAccess, CGPreflightPostEventAccess, CGRequestListenEventAccess, CGRequestPostEventAccess};

    const fn state(ok: bool) -> &'static str {
        if ok { "ok" } else { "missing" }
    }

    /// Ein Paket an die mDNS-Gruppe: löst die Abfrage «Lokales Netzwerk» aus; EHOSTUNREACH heisst gesperrt.
    fn local_network() -> &'static str {
        let Ok(s) = std::net::UdpSocket::bind("0.0.0.0:0") else { return "unknown" };
        match s.send_to(&[0u8; 12], "224.0.0.251:5353") {
            Err(e) if e.raw_os_error() == Some(65) => "missing",
            _ => "unknown",
        }
    }

    pub fn list() -> Vec<Perm> {
        vec![
            Perm { id: "local-network", label: "Lokales Netzwerk", purpose: "Verbindungen zu Rechnern im eigenen Netz (SSH, RDP, VNC, Dateien).", state: local_network() },
            Perm { id: "accessibility", label: "Gerätesteuerung und Datenzugriff", purpose: "⌘Tab, ⌘Leertaste und ⌃Pfeiltasten an den Remotecomputer geben.", state: state(CGPreflightPostEventAccess()) },
            Perm { id: "input-monitoring", label: "Eingabeüberwachung", purpose: "Tastenkürzel abfangen, solange das Remote-Bild aktiv ist.", state: state(CGPreflightListenEventAccess()) },
        ]
    }

    pub fn request(id: &str) {
        match id {
            "local-network" => drop(local_network()),
            "accessibility" => drop(CGRequestPostEventAccess()),
            "input-monitoring" => drop(CGRequestListenEventAccess()),
            _ => {}
        }
    }

    pub fn open(id: &str) -> Result<(), String> {
        let anchor = match id {
            "local-network" => "Privacy_LocalNetwork",
            "accessibility" => "Privacy_Accessibility",
            "input-monitoring" => "Privacy_ListenEvent",
            _ => return Err("Unbekannte Freigabe".into()),
        };
        std::process::Command::new("open")
            .arg(format!("x-apple.systempreferences:com.apple.preference.security?{anchor}"))
            .status()
            .map_err(|e| e.to_string())
            .map(|_| ())
    }
}

/// Windows und Linux: Sesam verbindet nur nach aussen, die Firewall fragt dafür nicht; nichts einzurichten.
#[cfg(not(target_os = "macos"))]
mod imp {
    use super::Perm;
    pub fn list() -> Vec<Perm> {
        Vec::new()
    }
    pub fn request(_: &str) {}
    pub fn open(_: &str) -> Result<(), String> {
        Err("Auf diesem System gibt es dafür keine Einstellung".into())
    }
}

pub use imp::{list, open, request};
