# Architektur

Tauri 2: Rust-Kern (`src-tauri/`) für alle Netzwerkprotokolle und Geheimnisse, schlankes
Web-Frontend (`src/`, Vite + TypeScript ohne Framework) nur für die Darstellung.
Geheimnisse verlassen den Rust-Kern nie in Richtung Frontend: das Frontend schreibt sie
in den Tresor, liest sie aber nicht zurück.

```
Frontend (TS)                       Rust-Kern
─────────────                       ─────────
main.ts   Bibliothek, Sheet  ──►  library_*  ─► library.rs ─► library.json (ohne Geheimnisse)
                                               └► vault.rs  ─► OS-Schlüsselbund
shell.ts  xterm.js          ◄──►  shell_*    ─► ssh.rs (russh)
desktop.ts canvas           ◄──►  vnc_*      ─► vnc/ (RFB)
(später)  Dateien 2-spaltig ◄──►  sftp_*     ─► sftp.rs (russh-sftp)
```

Laufende Daten (Terminal-Ausgabe, später Bildschirm-Updates) gehen über
`tauri::ipc::Channel` ans Frontend, nicht über globale Events.

## Module

### library (fertig)
`Connection` als JSON-Liste in `<App-Datenordner>/library.json`. Schreiben atomar über
Temp-Datei und Umbenennen. Eingaben werden im Kern geprüft (`Connection::validate`).
Die Felder `hasPassword`/`hasKey` sind nur Merker für die Anzeige.

### vault (fertig)
Crate `keyring` 4 (Modus `v1`): Windows-Anmeldeinformationsverwaltung, macOS-Schlüsselbund.
Dienst `ch.sesam.remote`, Konto `<id>:password` bzw. `<id>:ssh-key`.
Grenzen: Windows erlaubt höchstens 2560 Byte pro Eintrag (ed25519 passt, RSA-4096 nicht).
`keyring` v1 unterstützt **kein iOS**; für das iPad direkt `keyring-core` mit
`apple-native-keyring-store` (Feature `protected`) einbinden.

### ssh (fertig: Shell)
`russh` 0.65 mit `ring` statt `aws-lc-rs` (baut unter Windows ohne CMake/NASM).
Anmeldung: zuerst Schlüssel aus dem Tresor (Passwort dient bei Bedarf als Passphrase),
dann Passwort. Host-Schlüssel gegen `~/.ssh/known_hosts`: geänderter Schlüssel bricht ab,
unbekannter Host wird gelernt (wie OpenSSH `accept-new`, Bestätigung im UI folgt).

### sftp (geplant)
`russh-sftp` 3 auf derselben SSH-Verbindung (Subsystem `sftp`). Zweispaltige Ansicht,
Ziehen und Ablegen aus dem Explorer/Finder über Tauri-Drag-Drop-Events.

### clipboard (geplant)
Text: VNC `ClientCutText`/`ServerCutText`, mit Extended Clipboard (Pseudo-Encoding
0xC0A1E5CE) für UTF-8; ohne Erweiterung nur Latin-1. Dateien/Bilder: über SFTP in ein
Austauschverzeichnis, nur wenn SSH aktiv ist. Lokale Zwischenablage über
`tauri-plugin-clipboard-manager`.

### vnc (Meilenstein 2)
Eigener RFB-Client in `src-tauri/src/vnc/`:
- `mod.rs`: Handshake (3.3/3.7/3.8, macOS 003.889), Sitzung mit Lese- und Schreib-Task.
- `auth.rs`: VNC-Passwort (Typ 2, DES) und Apple (Typ 30, DH + AES-128 + MD5).
- `decode.rs`: Bildspeicher RGBA, Raw, CopyRect, ZRLE (ein zlib-Strom pro Verbindung).
- `clipboard.rs`: Extended Clipboard (UTF-8), Rückfall auf Latin-1.

Pixelformat fest 32 bpp/24 Bit, R/G/B-Shift 0/8/16: im Speicher R,G,B,X, also fast schon RGBA.
Geänderte Rechtecke gehen als Binärrahmen über `ipc::Channel` ans Frontend
(`0` Rechteck, `1` Grösse, `3` Ende, `4` Glocke, `5` Hinweis) und werden mit `putImageData` gezeichnet.
Die Zwischenablage gleicht der Rust-Kern direkt mit dem Betriebssystem ab
(`tauri-plugin-clipboard-manager`): Server → lokal sofort, lokal → Server beim Fokussieren.

Noch offen: VeNCrypt/TLS (wayvnc mit Anmeldung), Cursor-Pseudo-Encoding (lokaler Mauszeiger),
Tight/JPEG, VNC durch einen SSH-Tunnel, eigenes VNC-Passwort neben dem Kontopasswort.

## VNC: Crate-Entscheid

Anforderungen, die über «Bild zeigen» hinausgehen:

1. **Apple-Authentifizierung (Typ 30, Diffie-Hellman + AES)** und VNC-Auth (Typ 2) für
   die macOS-Bildschirmfreigabe.
2. **ExtendedDesktopSize / SetDesktopSize** für «An Fenster anpassen» (wayvnc und GNOME
   Remote Desktop unterstützen das).
3. **Extended Clipboard** für UTF-8-Text.
4. Kodierungen: mindestens Raw, CopyRect, Zlib, Tight oder ZRLE (Bandbreite), Cursor-Pseudo-Encoding.
5. Später iOS: reines Rust ohne C-Bibliotheken.

`vnc-rs` (0.6): reines Rust, async (tokio), Raw/CopyRect/Tight/Trle/Zrle, VNC-Auth.
Es fehlen Apple-Auth (Typ 30), SetDesktopSize und Extended Clipboard; die Auth- und
Nachrichten-Typen sind nicht erweiterbar, wir müssten also forken.

**Entscheid: eigene RFB-Implementierung im Modul `vnc/`**, angelehnt an
RFC 6143. Der Protokollkern (Handshake, Auth, Nachrichten, Raw/CopyRect/ZRLE) ist
überschaubar (~1500 Zeilen), und genau die Teile, die wir brauchen (Typ 30,
ExtendedDesktopSize, Extended Clipboard), müssten wir bei `vnc-rs` ohnehin selbst bauen.
`vnc-rs` dient als Referenz für die Dekoder.

Ursprünglicher Spike-Plan:
1. Handshake + VNC-Auth (Typ 2, DES) + Raw/CopyRect gegen wayvnc, Bild in `<canvas>`.
2. Apple-Auth Typ 30 gegen macOS (DH-Schlüsseltausch, AES-128-ECB über MD5 des Schlüssels).
3. ZRLE + Cursor-Pseudo-Encoding, dann ExtendedDesktopSize/SetDesktopSize.
4. Bildübertragung ans Frontend: Rechtecke als Binärdaten über `ipc::Channel`
   (`Response` mit `Vec<u8>`), Zeichnen mit `putImageData`.

## RDP: Crate-Entscheid (Meilenstein 4)

Anlass: Ubuntu 26.04 / GNOME 50 ist reines Wayland; GNOME Remote Desktop bietet nur noch RDP,
als «Remote Login» (Systemdienst, Anmeldebildschirm) und als Freigabe der laufenden Sitzung.

**Entscheid: IronRDP** (Devolutions, Rust, MIT/Apache-2.0), aufgeteilt in einzelne Crates:

| Crate | Version | Wofür |
|---|---|---|
| `ironrdp-connector` | 0.12 | Verbindungsaufbau, NLA (CredSSP über `sspi`, NTLM), Server-Redirection |
| `ironrdp-tokio` / `-async` | 0.12 | async Ablauf über tokio |
| `ironrdp-tls` | 0.3 | TLS mit rustls + ring, Rückruf für eigene Zertifikatsprüfung (Pinning) |
| `ironrdp-session` | 0.13 | aktive Sitzung, Bitmap/RemoteFX, setzt EGFX-Ausgabe ins Bild |
| `ironrdp-egfx` | 0.5 | Graphics Pipeline: RemoteFX Progressive, ClearCodec, Planar (H.264 nur mit OpenH264) |
| `ironrdp-displaycontrol` | 0.10 | Auflösung folgt dem Fenster |
| `ironrdp-input` | 0.9 | Scancodes, Unicode-Tasten, Maus |

Gründe: reines Rust (baut unter Windows ohne C-Bibliotheken, später iOS), aktiv gepflegt,
EGFX mit RemoteFX Progressive (GNOME verlangt die Graphics Pipeline), Server-Redirection im Connector.
Alternative FreeRDP: in C, über FFI und mit grossem Bau-Aufwand auf allen Zielen.

Umsetzung in `src-tauri/src/rdp.rs`; Bild und Ereignisse laufen über dieselben Binärrahmen wie VNC,
die Anzeige (`desktop.ts`) ist für beide Protokolle dieselbe.

- **Zertifikat:** Gültige Kette akzeptiert rustls; sonst entscheidet der gepinnte SHA256 (`known_certs.json`),
  beim ersten Mal nach Bestätigung im UI.
- **Server-Redirection (GNOME-Übergabe):** Nach dem Login am Anmeldebildschirm schickt GNOME eine
  Redirection-PDU. Sesam verbindet neu mit Routing-Token, Session-ID und Passwort-Cookie. Wie FreeRDP
  nimmt Sesam das Passwort aus der Umleitung auch für NLA. Höchstens 8 Umleitungen.
- **Tastatur:** physische Tasten als Scancodes (das Layout wählt der Server); mit AltGr erzeugte Zeichen
  und Tasten ohne Scancode als Unicode-Ereignisse.
- **Passwort:** eigenes RDP-Passwort im Tresor, sonst das Passwort des Benutzers; Domäne optional.

- **Zwischenablage:** `ironrdp-cliprdr` mit dem Windows-Backend aus `ironrdp-cliprdr-native` (Text, Bilder,
  Dateien mit verzögerter Bereitstellung). Es braucht ein Fenster mit Nachrichtenschleife und lebt darum
  auf dem Tauri-Hauptthread, je Sitzung eines. macOS hat noch kein Backend.
- **Ton:** `ironrdp-rdpsnd` statisch und dynamisch (`AUDIO_PLAYBACK_DVC`), Ausgabe über cpal, nur PCM
  (ohne Opus/AAC, damit keine C-Bibliotheken nötig sind).

- **Laufwerk:** `ironrdp-rdpdr` mit dem nativen Backend aus `ironrdp-rdpdr-native`; ein lokaler Ordner pro
  Verbindung (Sheet, standardmässig aus), pro Verbindung ein frisches Backend.
- **Mehrere Monitore:** «Alle Monitore» legt das Fenster rahmenlos über alle Monitore und schickt die
  Anordnung per Display Control (`encode_monitor_layout`, Hauptmonitor bei 0,0). Der Server richtet die
  Monitore ein; die Anzeige skaliert die Gesamtfläche deckungsgleich. Bei gemischter Skalierung der
  Monitore kann das Bild leicht unscharf werden (ein Fenster, eine Pixeldichte).

Noch offen: H.264 (OpenH264), Zwischenablage unter macOS, Drucker. Ein Ende-zu-Ende-Test gegen `ironrdp-server` ist vorbereitet,
braucht aber den Server aus dem IronRDP-Git-Repository (die veröffentlichte Version passt nicht zu den
Client-Crates).

## Plattformen

Zuerst Windows. macOS ist ohne Code-Änderung zu erwarten. iPad (`tauri ios init`) braucht
macOS mit Xcode und den iOS-Tresor (siehe vault).

## SSH: Umfang und Grenzen

Ziel ist der Umfang von PuTTY/PSCP, soweit `russh` 0.65 (Features `ring`, `rsa`, `flate2`, `des`)
und `russh-sftp` 3 ihn hergeben. Kryptografie implementiert Sesam nie selbst.

**Algorithmen** (`ssh::config`, Reihenfolge = Vorliebe des Clients):
- Schlüsseltausch: `mlkem768x25519-sha256` (Post-Quanten-Hybrid), `curve25519-sha256`,
  `ecdh-sha2-nistp256/384/521`, `diffie-hellman-group-exchange-sha256`, `group16/18-sha512`,
  `group14-sha256`; dazu immer `ext-info-c` und `kex-strict-c-v00@openssh.com` (Terrapin-Schutz).
- Chiffren: `chacha20-poly1305`, `aes256/128-gcm`, `aes256/192/128-ctr`.
- MACs: `hmac-sha2-512/256` (ETM zuerst).
- Host-Schlüssel: ed25519, ECDSA (P-256/384/521), RSA mit SHA-512/256.
- «Veraltete Algorithmen erlauben» hängt hinten an: `diffie-hellman-group14-sha1`,
  `group-exchange-sha1`, `group1-sha1`, `aes*-cbc`, `3des-cbc`, `hmac-sha1(-etm)`, `ssh-rsa` (SHA-1).
- Kompression `zlib@openssh.com`/`zlib` nur, wenn eingeschaltet; Keepalive in Sekunden (russh
  trennt nach drei unbeantworteten).

**Dateiübertragung:** SFTP (`russh-sftp`), sonst SCP wie PSCP (`scp.rs`, Einstellung «Dateiübertragung»:
Automatisch / Nur SFTP / Nur SCP). SCP läuft über exec-Kanäle: `scp -r -t`/`scp -r -f` für die Daten,
`ls -lAL`, `mkdir`, `test -e`, `mv`, `rm -f` für Auflisten, freie Namen und Teildateien. Pfade werden in
einfache Anführungszeichen gesetzt, Namen mit Zeilenumbruch abgelehnt, Namen vom Server dürfen weder `/`
noch `\` noch `..` enthalten. Mit SCP gibt es kein Umbenennen, Löschen und keine Rechte; Änderungsdatum
fehlt in der Liste, die Gesamtgrösse von Ordnern beim Herunterladen wächst mit (SCP nennt sie nicht vorab).
Die geteilte Zwischenablage (VNC) nutzt weiterhin nur SFTP.

**Schlüssel:** ed25519 und ECDSA, OpenSSH-Format oder PuTTY `.ppk` v2/v3 (über `ssh-key`, auch mit
Passphrase; Passphrase = Passwort der Verbindung). RSA nicht, weil Windows Geheimnisse als UTF-16 mit
höchstens 2560 Byte speichert (1280 Zeichen; RSA-2048 im OpenSSH-Format hat rund 1800).

**Agent:** «SSH-Agent verwenden» probiert zuerst Pageant, dann `\\.\pipe\openssh-ssh-agent`
(unter macOS/Linux `SSH_AUTH_SOCK`). Agent-Weiterleitung auf Wunsch, nur für die Shell; Agent-Kanäle
ohne eigene Anfrage werden abgelehnt.

**Portweiterleitung** (`forward.rs`, nur solange eine Shell offen ist): `L` lokal über direct-tcpip,
`D` SOCKS5 (ohne Anmeldung, nur CONNECT; kein SOCKS4), `R` über `tcpip-forward` auf 127.0.0.1 des Servers.
Lokale Ports lauschen nur auf 127.0.0.1 (IPv4). Meldungen erscheinen grau im Terminal.

**Sprungserver** (ProxyJump): eine andere Verbindung der Bibliothek; Sesam verbindet dorthin, öffnet
direct-tcpip zum Ziel und spricht SSH darüber (`connect_stream`). Nur eine Stufe. Der Host-Schlüssel des
Sprungservers muss schon bekannt sein (einmal direkt verbinden), die Rückfrage gibt es nur fürs Ziel.

**Nicht verfügbar** (russh kann es nicht oder nur mit eigenem Krypto-Code):
`sntrup761x25519-sha512` (fehlt in russh), `arcfour`, `blowfish`, DSA-Host-Schlüssel
(nur mit Feature `dsa`, bewusst aus), GSSAPI/Kerberos (russh hat nur die Schnittstelle
`authenticate_gssapi_with_mic`; SSPI-Anbindung wäre eigener Code), Server-Zertifikate (`*-cert-v01`,
werden abgelehnt), SSH-1, X11-Weiterleitung, Weiterleitung über Unix-Sockets, keyboard-interactive/2FA
(russh kann es, hier noch ohne Dialog), serielle Verbindungen, Telnet/rlogin.
