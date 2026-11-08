//! Erreichbarkeit (Online-Punkt mit Latenz in der Bibliothek) und Wake-on-LAN.
use std::net::{SocketAddr, ToSocketAddrs, UdpSocket};
use std::time::{Duration, Instant};

/// TCP-Verbindungsaufbau zum ersten erreichbaren Port; Latenz in ms, `None` = nicht erreichbar.
pub async fn reach(host: &str, ports: &[u16]) -> Option<u32> {
    for &port in ports {
        let t = Instant::now();
        let ok = tokio::time::timeout(Duration::from_millis(1500), tokio::net::TcpStream::connect((host, port))).await;
        if matches!(ok, Ok(Ok(_))) {
            return Some(t.elapsed().as_millis().min(u128::from(u32::MAX)) as u32);
        }
    }
    None
}

/// MAC-Adresse aus der ARP-Tabelle (nach einem Kontakt mit dem Rechner im selben Netz). Ohne Fremdpakete:
/// Windows `arp -a <ip>`, sonst `arp -n <ip>` bzw. `ip neigh`.
pub fn mac_of(host: &str) -> Option<String> {
    let ip = (host, 0).to_socket_addrs().ok()?.find(|a| a.is_ipv4())?.ip().to_string();
    let run = |cmd: &str, args: &[&str]| {
        let mut c = std::process::Command::new(cmd);
        c.args(args);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            c.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
        }
        c.output().ok().map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
    };
    let out = if cfg!(windows) { run("arp", &["-a", &ip])? } else { run("arp", &["-n", &ip]).or_else(|| run("ip", &["neigh", "show", &ip]))? };
    parse_mac(&out, &ip)
}

/// Erste MAC-Adresse in der Zeile mit `ip` (Formate `aa-bb-…` und `aa:bb:…`), normiert als `AA:BB:CC:DD:EE:FF`.
fn parse_mac(text: &str, ip: &str) -> Option<String> {
    text.lines().filter(|l| l.split_whitespace().any(|w| w.trim_matches(|c| c == '(' || c == ')') == ip)).find_map(|l| {
        l.split_whitespace().find_map(|w| {
            let parts: Vec<&str> = w.split(['-', ':']).collect();
            (parts.len() == 6 && parts.iter().all(|p| p.len() == 2 && u8::from_str_radix(p, 16).is_ok()) && w != "ff-ff-ff-ff-ff-ff")
                .then(|| parts.join(":").to_uppercase())
        })
    })
}

/// Magic Packet: 6 × 0xFF, dann 16 × die MAC.
fn magic_packet(mac: &str) -> Result<Vec<u8>, String> {
    let bytes: Vec<u8> = mac.split([':', '-']).map(|p| u8::from_str_radix(p, 16)).collect::<Result<_, _>>().map_err(|_| "Ungültige MAC-Adresse".to_string())?;
    if bytes.len() != 6 {
        return Err("Ungültige MAC-Adresse".into());
    }
    let mut p = vec![0xFF; 6];
    for _ in 0..16 {
        p.extend_from_slice(&bytes);
    }
    Ok(p)
}

/// Magic Packet als Broadcast (Port 9 und 7) ins lokale Netz.
pub fn wake(mac: &str) -> Result<(), String> {
    let packet = magic_packet(mac)?;
    let sock = UdpSocket::bind("0.0.0.0:0").map_err(|e| e.to_string())?;
    sock.set_broadcast(true).map_err(|e| e.to_string())?;
    for port in [9, 7] {
        sock.send_to(&packet, SocketAddr::from(([255, 255, 255, 255], port))).map_err(|e| format!("Wake-on-LAN: {e}"))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mac_aus_arp_tabelle() {
        let win = "\nSchnittstelle: 192.168.1.20 --- 0x7\n  Internetadresse       Physische Adresse     Typ\n  192.168.1.4           a4-bb-6d-01-02-03     dynamisch\n";
        assert_eq!(parse_mac(win, "192.168.1.4").as_deref(), Some("A4:BB:6D:01:02:03"));
        let linux = "? (192.168.1.4) at a4:bb:6d:01:02:03 [ether] on eth0";
        assert_eq!(parse_mac(linux, "192.168.1.4").as_deref(), Some("A4:BB:6D:01:02:03"));
        assert_eq!(parse_mac(win, "192.168.1.5"), None);
    }

    #[test]
    fn magic_packet_aufbau() {
        let p = magic_packet("a4:bb:6d:01:02:03").unwrap();
        assert_eq!(p.len(), 6 + 16 * 6);
        assert!(p[..6].iter().all(|&b| b == 0xFF));
        assert_eq!(&p[6..12], &[0xA4, 0xBB, 0x6D, 0x01, 0x02, 0x03]);
        assert!(magic_packet("zz").is_err());
    }
}
