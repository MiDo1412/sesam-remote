//! Verbindungen übernehmen: OpenSSH-Konfiguration (`~/.ssh/config`), Remotedesktop-Dateien (`.rdp`) und Geräte,
//! die sich im lokalen Netz per Bonjour/mDNS melden. Ergebnis sind Vorschläge; gespeichert wird erst in der Oberfläche.

use crate::library::Connection;
use serde::Serialize;

/// Ein Vorschlag. `jump` ist der Host-Alias des Sprungservers aus derselben Datei, `key_file` der Pfad aus
/// `IdentityFile` (die Oberfläche liest ihn mit `read_key_file`, also nur gültige Schlüssel).
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Found {
    pub connection: Connection,
    pub jump: String,
    pub key_file: String,
}

fn blank(name: &str, address: &str) -> Connection {
    Connection {
        name: name.into(),
        address: address.into(),
        vnc_port: None,
        ssh_port: None,
        rdp_port: None,
        ftp_port: None,
        ..Default::default()
    }
}

/// `*` und `?` wie in ssh_config(5).
fn glob(pat: &str, s: &str) -> bool {
    fn m(p: &[u8], s: &[u8]) -> bool {
        match (p.first(), s.first()) {
            (None, None) => true,
            (Some(b'*'), _) => m(&p[1..], s) || (!s.is_empty() && m(p, &s[1..])),
            (Some(b'?'), Some(_)) => m(&p[1..], &s[1..]),
            (Some(a), Some(b)) if a.eq_ignore_ascii_case(b) => m(&p[1..], &s[1..]),
            _ => false,
        }
    }
    m(pat.as_bytes(), s.as_bytes())
}

/// Wörter einer Zeile, `"…"` hält Leerzeichen zusammen; `Key=Wert` geht auch.
fn words(line: &str) -> Vec<String> {
    let line = line.trim();
    let (key, rest) = match line.find(|c: char| c.is_whitespace() || c == '=') {
        Some(i) => (&line[..i], line[i..].trim_start_matches(|c: char| c.is_whitespace() || c == '=')),
        None => (line, ""),
    };
    let mut out = vec![key.to_ascii_lowercase()];
    let (mut cur, mut quoted) = (String::new(), false);
    for c in rest.chars() {
        match c {
            '"' => quoted = !quoted,
            c if c.is_whitespace() && !quoted => {
                if !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
            }
            c => cur.push(c),
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// OpenSSH-Konfiguration lesen. Je konkretem `Host`-Alias gelten alle passenden Blöcke in Dateireihenfolge, der erste
/// Wert gewinnt (wie bei ssh). `Match` und `Include` werden übersprungen.
pub fn ssh_config(text: &str, home: &str) -> Vec<Found> {
    // (Muster, Einstellungen) je Block; Einstellungen vor dem ersten Host gelten für alle.
    let mut blocks: Vec<(Vec<String>, Vec<Vec<String>>)> = vec![(vec!["*".into()], Vec::new())];
    let mut skip = false;
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let w = words(line);
        match w[0].as_str() {
            "host" => {
                skip = false;
                blocks.push((w[1..].to_vec(), Vec::new()));
            }
            "match" => skip = true,
            _ if !skip && w.len() > 1 => blocks.last_mut().unwrap().1.push(w),
            _ => {}
        }
    }
    let aliases: Vec<String> = blocks
        .iter()
        .flat_map(|(p, _)| p.iter())
        .filter(|a| !a.contains(['*', '?', '!']))
        .cloned()
        .fold(Vec::new(), |mut v, a| {
            if !v.contains(&a) {
                v.push(a);
            }
            v
        });
    let expand = |p: &str| p.strip_prefix("~/").map_or(p.to_string(), |r| format!("{home}/{r}"));
    aliases
        .iter()
        .map(|alias| {
            let get = |key: &str| -> Option<&str> {
                blocks
                    .iter()
                    .filter(|(pats, _)| pats.iter().any(|p| glob(p, alias)) && !pats.iter().any(|p| p.strip_prefix('!').is_some_and(|p| glob(p, alias))))
                    .flat_map(|(_, opts)| opts.iter())
                    .find(|o| o[0] == key)
                    .map(|o| o[1].as_str())
            };
            let yes = |key: &str| get(key).is_some_and(|v| v.eq_ignore_ascii_case("yes"));
            let mut c = blank(alias, get("hostname").unwrap_or(alias).replace("%h", alias).as_str());
            c.ssh_port = Some(get("port").and_then(|p| p.parse().ok()).unwrap_or(22));
            c.user = get("user").unwrap_or_default().into();
            c.default_view = "shell".into();
            c.agent_forward = yes("forwardagent");
            c.ssh_agent = c.agent_forward;
            c.ssh_compression = yes("compression");
            c.ssh_keepalive = get("serveraliveinterval").and_then(|v| v.parse().ok()).unwrap_or(0);
            // Nur die erste Stufe, und nur wenn sie selbst ein Alias aus dieser Datei ist.
            let jump = get("proxyjump").and_then(|j| j.split(',').next()).filter(|j| aliases.iter().any(|a| a == j) && *j != alias).unwrap_or_default();
            let key_file = get("identityfile").map(expand).unwrap_or_default();
            Found { connection: c, jump: jump.into(), key_file }
        })
        .collect()
}

/// Remotedesktop-Datei (mstsc, Azure, RD Web). mstsc speichert UTF-16 mit BOM.
pub fn rdp_file(bytes: &[u8], name: &str) -> Result<Found, String> {
    let text = match bytes {
        [0xFF, 0xFE, rest @ ..] => String::from_utf16_lossy(&rest.chunks_exact(2).map(|b| u16::from_le_bytes([b[0], b[1]])).collect::<Vec<_>>()),
        [0xEF, 0xBB, 0xBF, rest @ ..] => String::from_utf8_lossy(rest).into_owned(),
        _ => String::from_utf8_lossy(bytes).into_owned(),
    };
    let mut kv = std::collections::HashMap::new();
    for line in text.lines() {
        let mut p = line.trim().splitn(3, ':');
        if let (Some(k), Some(_), Some(v)) = (p.next(), p.next(), p.next()) {
            kv.insert(k.trim().to_ascii_lowercase(), v.trim().to_string());
        }
    }
    let full = kv.get("full address").or(kv.get("alternate full address")).filter(|a| !a.is_empty()).ok_or("Keine Remotedesktop-Datei (es fehlt «full address»)")?;
    // `host:port`, IPv6 als `[::1]:port`
    let (host, port) = match full.rsplit_once(':') {
        Some((h, p)) if (!h.contains(':') || h.ends_with(']')) && p.parse::<u16>().is_ok() => (h.trim_matches(['[', ']']).to_string(), p.parse().ok()),
        _ => (full.trim_matches(['[', ']']).to_string(), None),
    };
    let mut c = blank(name, &host);
    c.rdp_port = Some(port.or_else(|| kv.get("server port").and_then(|p| p.parse().ok())).unwrap_or(3389));
    c.desktop_protocol = "rdp".into();
    c.default_view = "desktop".into();
    let user = kv.get("username").cloned().unwrap_or_default();
    match user.split_once('\\') {
        Some((d, u)) => (c.rdp_domain, c.rdp_user) = (d.into(), u.into()),
        None => (c.rdp_domain, c.rdp_user) = (kv.get("domain").cloned().unwrap_or_default(), user),
    }
    // gatewayusagemethod 0 = nie, 4 = nur wenn nötig (lokal meist direkt); 1/2 = immer bzw. automatisch.
    if let Some(gw) = kv.get("gatewayhostname").filter(|g| !g.is_empty() && kv.get("gatewayusagemethod").is_none_or(|m| m != "0")) {
        c.rdp_gateway = gw.clone();
    }
    Ok(Found { connection: c, jump: String::new(), key_file: String::new() })
}

/// Gerät aus der Netzwerksuche.
#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Seen {
    pub name: String,
    pub host: String,
    pub address: String,
    pub port: u16,
    /// ssh, vnc oder rdp
    pub service: String,
}

/// Bonjour/mDNS: `_ssh._tcp`, `_rfb._tcp` (VNC, auch macOS-Bildschirmfreigabe), `_rdp._tcp` einige Sekunden lang.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub fn discover(secs: u64) -> Result<Vec<Seen>, String> {
    use mdns_sd::{ServiceDaemon, ServiceEvent};
    let d = ServiceDaemon::new().map_err(|e| format!("Netzwerksuche nicht möglich: {e}"))?;
    let types = [("_ssh._tcp.local.", "ssh"), ("_sftp-ssh._tcp.local.", "ssh"), ("_rfb._tcp.local.", "vnc"), ("_rdp._tcp.local.", "rdp")];
    let rx: Vec<_> = types.iter().filter_map(|(t, s)| d.browse(t).ok().map(|r| (r, *s))).collect();
    let until = std::time::Instant::now() + std::time::Duration::from_secs(secs);
    let mut out: Vec<Seen> = Vec::new();
    while std::time::Instant::now() < until {
        for (r, service) in &rx {
            while let Ok(ev) = r.try_recv() {
                let ServiceEvent::ServiceResolved(info) = ev else { continue };
                let host = info.get_hostname().trim_end_matches('.').to_string();
                let address = info.get_addresses_v4().into_iter().next().map(|a| a.to_string()).unwrap_or_else(|| host.clone());
                let name = mdns_sd::instance_name_unescaped(info.get_fullname());
                let s = Seen { name, host, address, port: info.get_port(), service: (*service).into() };
                if !out.iter().any(|o| o.address == s.address && o.port == s.port && o.service == s.service) {
                    out.push(s);
                }
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    let _ = d.shutdown();
    out.sort_by_key(|a| a.name.to_lowercase());
    Ok(out)
}

#[cfg(any(target_os = "ios", target_os = "android"))]
pub fn discover(_secs: u64) -> Result<Vec<Seen>, String> {
    Err("Die Netzwerksuche gibt es auf diesem Gerät nicht.".into())
}

#[cfg(test)]
mod tests {
    #[test]
    fn ssh_config_wie_openssh() {
        let cfg = r#"
# Kommentar
User global
Host web prod-*
    HostName 10.0.0.5
    Port 2222
    IdentityFile ~/.ssh/id_web
Host bastion
    HostName bastion.example.com
    User ops
    ForwardAgent yes
Host db
    HostName=db.internal
    ProxyJump bastion,other
    ServerAliveInterval 30
Match host *.corp
    User falsch
Host *
    User fallback
    Compression yes
"#;
        let f = super::ssh_config(cfg, "/home/m");
        let names: Vec<_> = f.iter().map(|x| x.connection.name.as_str()).collect();
        assert_eq!(names, ["web", "bastion", "db"]);
        let web = &f[0];
        assert_eq!((web.connection.address.as_str(), web.connection.ssh_port, web.connection.user.as_str()), ("10.0.0.5", Some(2222), "global"));
        assert_eq!(web.key_file, "/home/m/.ssh/id_web");
        assert!(web.connection.ssh_compression && web.connection.rdp_port.is_none() && web.connection.vnc_port.is_none());
        assert_eq!((f[1].connection.user.as_str(), f[1].connection.agent_forward), ("global", true));
        assert_eq!((f[2].connection.address.as_str(), f[2].jump.as_str(), f[2].connection.ssh_keepalive), ("db.internal", "bastion", 30));
        // ohne «User» vor dem ersten Host gilt der Wert aus «Host *»
        assert_eq!(super::ssh_config("Host a\nHost *\n User fb\n", "/h")[0].connection.user, "fb");
    }

    /// Echte Netzwerksuche (braucht Multicast im lokalen Netz).
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    #[test]
    #[ignore]
    fn netzwerksuche_laeuft() {
        // Eigenen Dienst anmelden und wiederfinden.
        let d = mdns_sd::ServiceDaemon::new().unwrap();
        let ip = std::env::var("SESAM_TEST_IP").unwrap_or("127.0.0.1".into());
        let info = mdns_sd::ServiceInfo::new("_ssh._tcp.local.", "Sesam Test", "sesam-test.local.", ip.as_str(), 2299, None::<std::collections::HashMap<String, String>>).unwrap();
        d.register(info).unwrap();
        let seen = super::discover(3).unwrap();
        println!("{seen:?}");
        assert!(seen.iter().any(|s| s.name == "Sesam Test" && s.port == 2299 && s.service == "ssh"));
        let _ = d.shutdown();
    }

    #[test]
    fn rdp_datei_auch_utf16() {
        let txt = "screen mode id:i:2\r\nfull address:s:pc01.example.com:3390\r\nusername:s:FIRMA\\anna\r\ngatewayhostname:s:gw.example.com\r\ngatewayusagemethod:i:1\r\n";
        let mut bytes = vec![0xFF, 0xFE];
        bytes.extend(txt.encode_utf16().flat_map(|u| u.to_le_bytes()));
        let f = super::rdp_file(&bytes, "PC01").unwrap();
        let c = &f.connection;
        assert_eq!((c.address.as_str(), c.rdp_port, c.rdp_domain.as_str(), c.rdp_user.as_str(), c.rdp_gateway.as_str()), ("pc01.example.com", Some(3390), "FIRMA", "anna", "gw.example.com"));
        assert_eq!(c.desktop_protocol, "rdp");
        let f = super::rdp_file(b"full address:s:[fe80::1]\nusername:s:bob@corp.ch\ngatewayhostname:s:gw\ngatewayusagemethod:i:0\n", "v6").unwrap();
        assert_eq!((f.connection.address.as_str(), f.connection.rdp_port, f.connection.rdp_user.as_str(), f.connection.rdp_gateway.as_str()), ("fe80::1", Some(3389), "bob@corp.ch", ""));
        assert!(super::rdp_file(b"hallo", "x").is_err());
    }
}
