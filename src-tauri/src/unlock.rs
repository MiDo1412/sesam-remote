//! «Desktop freischalten»: Gerät mit nur SSH per SSH erkennen und den passenden Desktop-Dienst einschalten.
//! Die Befehle stehen hier fest (nicht vom Frontend), werden vorher gezeigt und erst nach Bestätigung mit sudo ausgeführt.

use serde::Serialize;

/// Nur Lesen: System, Version, Sitzungsart, vorhandene Werkzeuge, Modell (Raspberry Pi).
pub const DETECT: &str = "uname -s; (. /etc/os-release 2>/dev/null && echo \"id=$ID\" && echo \"version=$VERSION_ID\" && echo \"name=$PRETTY_NAME\"); \
command -v sw_vers >/dev/null 2>&1 && echo \"macos=$(sw_vers -productVersion)\"; \
printf 'model='; tr -d '\\0' < /proc/device-tree/model 2>/dev/null; echo; \
for t in grdctl raspi-config wayvnc vncserver-x11; do command -v $t >/dev/null 2>&1 && echo \"tool=$t\"; done; \
for s in $(loginctl list-sessions --no-legend 2>/dev/null | awk '{print $1}'); do echo \"session=$(loginctl show-session \"$s\" -p Type --value 2>/dev/null)\"; done; true";

#[derive(Serialize, Debug, PartialEq, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Plan {
    /// ubuntu, raspberry, macos oder "" (nicht unterstützt)
    pub system: String,
    /// Für Menschen: «Ubuntu 24.04 LTS mit GNOME» …
    pub label: String,
    /// "rdp" oder "vnc"
    pub protocol: String,
    pub port: u16,
    /// Was ausgeführt wird (zum Zeigen); bei Ubuntu startet das Setup-Script interaktiv in der Shell.
    pub commands: Vec<String>,
    pub note: String,
}

pub fn plan(out: &str) -> Plan {
    let get = |k: &str| out.lines().find_map(|l| l.trim().strip_prefix(k).map(|v| v.trim().to_string())).unwrap_or_default();
    let tools: Vec<&str> = out.lines().filter_map(|l| l.trim().strip_prefix("tool=")).collect();
    let (id, model, macos) = (get("id="), get("model="), get("macos="));
    let name = if get("name=").is_empty() { id.clone() } else { get("name=") };
    if !macos.is_empty() || out.lines().next().is_some_and(|l| l.trim() == "Darwin") {
        return Plan {
            system: "macos".into(),
            label: format!("macOS {macos}"),
            protocol: "vnc".into(),
            port: 5900,
            commands: lines("macos"),
            note: "Schaltet die Bildschirmfreigabe ein; anmelden mit einem Benutzerkonto des Macs. Je nach macOS-Version muss die Steuerung einmal am Mac selbst erlaubt werden (Systemeinstellungen › Allgemein › Teilen).".into(),
        };
    }
    if model.starts_with("Raspberry Pi") || id == "raspbian" || tools.contains(&"raspi-config") {
        let wayland = out.lines().any(|l| l.trim() == "session=wayland") || tools.contains(&"wayvnc");
        return Plan {
            system: "raspberry".into(),
            label: format!("{} ({})", if model.is_empty() { "Raspberry Pi".to_string() } else { model.clone() }, if wayland { "Wayland, wayvnc" } else { "X11, RealVNC" }),
            protocol: "vnc".into(),
            port: 5900,
            commands: lines("raspberry"),
            note: if wayland {
                "Schaltet VNC über raspi-config ein (wayvnc). Anmelden mit dem Benutzer des Pi.".into()
            } else {
                "Schaltet VNC über raspi-config ein (RealVNC Server). Anmelden mit dem Benutzer des Pi.".into()
            },
        };
    }
    if id == "ubuntu" && tools.contains(&"grdctl") {
        return Plan {
            system: "ubuntu".into(),
            label: format!("{name} mit GNOME"),
            protocol: "rdp".into(),
            port: 3389,
            commands: vec!["bash ~/sesam-rdp-setup.sh".into()],
            note: "Startet das Setup-Script von Sesam in einer Shell: Anmeldebildschirm per RDP, Zertifikate und Firewall. Es fragt jeden Schritt nach; Passwörter tippst du dort selbst ein.".into(),
        };
    }
    Plan {
        system: String::new(),
        label: if name.is_empty() { "Unbekanntes System".into() } else { name },
        protocol: String::new(),
        port: 0,
        commands: Vec::new(),
        note: "Für dieses System kennt Sesam keinen sicheren Weg, den Desktop per SSH einzuschalten. Unter Linux mit GNOME: «Einstellungen › System › Remote-Desktop», sonst z. B. xrdp oder wayvnc.".into(),
    }
}

/// Was mit sudo läuft, Zeile für Zeile (so auch angezeigt). Ubuntu läuft interaktiv in der Shell, nicht hier.
const MACOS: &[&str] = &[
    "launchctl enable system/com.apple.screensharing",
    "launchctl bootstrap system /System/Library/LaunchDaemons/com.apple.screensharing.plist 2>/dev/null || launchctl kickstart -k system/com.apple.screensharing",
];
const RASPBERRY: &[&str] = &["raspi-config nonint do_vnc 0"];

fn lines(system: &str) -> Vec<String> {
    script_lines(system).unwrap_or_default().iter().map(|l| l.to_string()).collect()
}

fn script_lines(system: &str) -> Result<&'static [&'static str], String> {
    match system {
        "macos" => Ok(MACOS),
        "raspberry" => Ok(RASPBERRY),
        _ => Err("Für dieses System führt Sesam nichts aus".into()),
    }
}

/// Die Zeilen als ein Script für `sudo sh -c`, jede muss gelingen.
pub fn script(system: &str) -> Result<String, String> {
    Ok(script_lines(system)?.iter().map(|l| format!("{{ {l}; }}")).collect::<Vec<_>>().join(" && "))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Gegen einen echten SSH-Server (nur auf Wunsch), wie in sftp.rs: SESAM_TEST_SSH="host:port:user:schlüsseldatei:…".
    #[tokio::test]
    #[ignore]
    async fn erkennung_echter_server() {
        let v = std::env::var("SESAM_TEST_SSH").expect("SESAM_TEST_SSH");
        let p: Vec<&str> = v.splitn(5, ':').collect();
        let mut t = crate::ssh::Target { host: p[0].into(), port: p[1].parse().unwrap(), user: p[2].into(), password: None, key: Some(std::fs::read_to_string(p[3]).unwrap()), trusted: None, opts: Default::default(), forwards: Vec::new(), jump: None };
        let out = match crate::ssh::exec(&t, DETECT).await {
            Ok(o) => o,
            Err(e) => {
                t.trusted = Some(e.rsplit('|').next().unwrap().to_string());
                crate::ssh::exec(&t, DETECT).await.unwrap()
            }
        };
        let plan = plan(&out);
        println!("{out}\n{plan:?}");
        assert!(!plan.label.is_empty());
        if cfg!(target_os = "macos") && p[0] == "127.0.0.1" {
            assert_eq!(plan.system, "macos");
        }
    }

    #[test]
    fn erkennt_systeme() {
        let pi = plan("Linux\nid=debian\nversion=12\nname=Debian GNU/Linux 12 (bookworm)\nmodel=Raspberry Pi 5 Model B Rev 1.0\ntool=raspi-config\ntool=wayvnc\nsession=wayland\n");
        assert_eq!((pi.system.as_str(), pi.protocol.as_str(), pi.port), ("raspberry", "vnc", 5900));
        assert!(pi.label.contains("Wayland") && pi.commands == ["raspi-config nonint do_vnc 0"]);
        assert_eq!(script("raspberry").unwrap(), "{ raspi-config nonint do_vnc 0; }");
        let x11 = plan("Linux\nid=raspbian\nmodel=Raspberry Pi 4 Model B\ntool=raspi-config\nsession=x11\n");
        assert!(x11.label.contains("X11"));
        let u = plan("Linux\nid=ubuntu\nversion=24.04\nname=Ubuntu 24.04.1 LTS\nmodel=\ntool=grdctl\nsession=wayland\n");
        assert_eq!((u.system.as_str(), u.protocol.as_str(), u.port), ("ubuntu", "rdp", 3389));
        let m = plan("Darwin\nmacos=15.5\nmodel=\n");
        assert_eq!((m.system.as_str(), m.label.as_str()), ("macos", "macOS 15.5"));
        let other = plan("Linux\nid=fedora\nname=Fedora Linux 41\nmodel=\n");
        assert!(other.system.is_empty() && other.commands.is_empty() && other.label == "Fedora Linux 41");
        assert!(script("raspberry").is_ok() && script("macos").is_ok() && script("ubuntu").is_err() && script("x; rm -rf /").is_err());
    }
}
