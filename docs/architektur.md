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
| `ironrdp-connector` | 0.12 | Verbindungsaufbau, NLA (CredSSP über `sspi`: Kerberos, NTLM), Server-Redirection |
| `ironrdp-tokio` / `-async` | 0.12 | async Ablauf über tokio |
| `ironrdp-tls` | 0.3 | TLS mit rustls + ring, Rückruf für eigene Zertifikatsprüfung (Pinning) |
| `ironrdp-session` | 0.13 | aktive Sitzung, Bitmap/RemoteFX, setzt EGFX-Ausgabe ins Bild |
| `ironrdp-egfx` | 0.5 | Graphics Pipeline: RemoteFX Progressive, ClearCodec, Planar, AVC420/AVC444 (Decoder des Systems) |
| `ironrdp-mstsgu` | 0.1 | RD-Gateway (MS-TSGU über HTTPS/WebSocket) |
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

Noch offen: H.264 unter macOS (VideoToolbox), Zwischenablage unter macOS, Drucker. Ein Ende-zu-Ende-Test gegen `ironrdp-server` ist vorbereitet,
braucht aber den Server aus dem IronRDP-Git-Repository (die veröffentlichte Version passt nicht zu den
Client-Crates).

## RDP: Anmeldung, Gateway, H.264 und Sicherheit

Stand nach dem Ausbau «aktuelle Standards» (Microsoft kündigt NTLM ab).

### Kerberos (NLA/CredSSP)

- **Ablauf:** CredSSP läuft in `rdp::credssp` (eigene Schleife statt in `connect_finalize`, sonst wie
  ironrdp-async). Mit Realm (Benutzer `benutzer@domain.tld` oder DNS-Domäne wie `contoso.local`), mit
  eingetragenem KDC oder gesperrtem NTLM übergibt Sesam eine `KerberosConfig`: sspi spricht dann SPNEGO
  (Negotiate) mit **Kerberos zuerst**. Ohne Realm (lokale Konten, GNOME, xrdp, NetBIOS-Domäne `CONTOSO`)
  bleibt es bei NTLM wie bisher, ohne KDC-Suche.
- **KDC finden:** macht sspi selbst: Umgebungsvariable `SSPI_KDC_URL`, Registry
  (`…\Lsa\Kerberos\Domains`), dann DNS-SRV `_kerberos._tcp.<realm>` über den Resolver des Betriebssystems
  (Windows `DnsQuery_W`, macOS dnssd). Darum braucht Sesam **keinen eigenen Resolver** (hickory o. ä.).
  Unter Linux fände sspi den KDC nur über krb5.conf oder sein Feature `dns_resolver`.
- **Transport** (`kdc.rs`): TCP (Längenpräfix), UDP und KDC-Proxy nach MS-KKDCP über HTTP(S). Die
  KDC-PROXY-MESSAGE (ASN.1) baut und entpackt sspi; Sesam schickt sie per `POST` mit
  `Content-Type: application/kerberos` (eigenes kleines HTTP/1.1, TLS über `ironrdp-tls` gegen die
  Stammzertifikate des Systems, ohne Pinning). Antworten höchstens 1 MiB, Zeitlimit 10 s.
- **Rückfall auf NTLM:** Ist der KDC nicht erreichbar oder kennt er Konto bzw. Server nicht, meldet Sesam
  sspi `NoAuthenticatingAuthority`, und sspi fällt auf NTLM zurück. Wählt der Server selbst NTLM, ebenso.
- **«NTLM nicht erlauben»** (Erweitert): sspi kennt zwar eine Paketliste (`!ntlm`), ironrdp-connector 0.12
  reicht sie aber nicht durch. Sesam prüft darum jedes ausgehende SPNEGO-Token auf die NTLM-Signatur
  `NTLMSSP\0` und bricht **vor dem Senden** ab. Eine NTLM-Antwort verlässt den Rechner so nie.
- **Eingaben pro Verbindung:** «Kerberos-KDC» leer = automatisch, sonst `host[:port]`, `tcp://`, `udp://`
  oder `https://gateway/KdcProxy`. Als Adresse muss der Servername stehen, der im SPN steht
  (`TERMSRV/<host>`); mit einer IP-Adresse klappt Kerberos nicht (dann NTLM oder Fehler).
- **Ungetestet** gegen einen echten AD-KDC und einen KDC-Proxy. Getestet: Eingabeformen, HTTP-Rahmen
  (Content-Length, chunked, Fehlerstatus), TCP-Rundlauf, NTLM-Erkennung, Entscheid Kerberos ja/nein.

**Mit aktuellem Windows-Konto anmelden (SSO):** mit sspi nicht machbar. sspi ist eine reine
Rust-Implementierung und braucht Benutzer und Passwort; an das TGT bzw. die Anmeldedaten der laufenden
Windows-Sitzung (LSA) kommt sie nicht heran. Möglich wäre es nur unter Windows mit der nativen SSPI
(`AcquireCredentialsHandleW`/`InitializeSecurityContextW`, Paket «Negotiate» mit Standard-Anmeldedaten)
anstelle von `CredSspClient`, also ein zweiter CredSSP-Pfad über das `windows`-Crate. Für die Weitergabe der
Anmeldedaten an den Server muss zudem die Richtlinie «Delegierung von Standardanmeldeinformationen
zulassen» mit `TERMSRV/*` gesetzt sein. Aufwand mittel, nur Windows, darum zurückgestellt.

### RD-Gateway (MS-TSGU)

`ironrdp-mstsgu` 0.1 (crates.io) passt zu den übrigen Versionen (ironrdp-tls 0.3, sspi 0.23) und ist
eingebaut: Feld «RD-Gateway» (`host[:port]`, Standard 443) unter Erweitert. «Gateway-Benutzer» leer =
gleiche Zugangsdaten wie die Verbindung (mit Domäne als `DOMÄNE\benutzer`), sonst eigener Benutzer mit
eigenem Gateway-Passwort im Tresor. Das Crate spricht HTTPS/WebSocket und fällt auf den alten
HTTP-Transport zurück; Anmeldung am Gateway über HTTP Negotiate (Kerberos, dann NTLM) oder Basic.
Das Gateway-Zertifikat prüft es strikt gegen die Stammzertifikate des Systems (wie mstsc, ohne
Pinning-Dialog); das Zertifikat des RDP-Servers dahinter wie gewohnt mit Pinning. Umleitungen während des
Aufbaus laufen wieder durch dasselbe Gateway.

Kosten: `ironrdp-mstsgu` zieht über `sspi/network_client` reqwest 0.12, hyper und **aws-lc-rs** mit
(baut hier ohne CMake/NASM). Weil damit zwei rustls-Provider im Baum sind, installiert `rdp.rs` ring als
Standard-Provider, bevor TLS beginnt. Nicht umgesetzt: RPC-over-HTTP (`RpchStream`), UDP-Seitenkanal,
Smartcard am Gateway, Anzeige der Consent-Meldung (wird automatisch akzeptiert), Gateway-Richtlinien
(Umleitungsverbote). **Ungetestet** gegen ein echtes Gateway.

### H.264 (AVC420/AVC444)

`GraphicsPipelineClient::new` erhält einen `H264Decoder` (`h264.rs`). Unter Windows ist das der
**Microsoft H.264 Video Decoder MFT** (Media Foundation über das `windows`-Crate 0.62, das ohnehin im Baum
ist): Eingabe Annex B (das AVC-Längenformat wird umgeschrieben), Ausgabe NV12, Low-Latency-Modus, damit
jedes Eingabebild sofort ein Ausgabebild liefert (AVC444 paart zwei aufeinanderfolgende Bilder). Sesam
gibt die YUV-Ebenen zurück (`decode_yuv420`); die Farbumrechnung nach MS-RDPEGFX und die
AVC444-Kombination macht ironrdp-egfx. Ohne Decoder (Windows N ohne Media Feature Pack) meldet
`wait_until_ready` false, und der Server bekommt kein AVC angeboten. Dekodiert wird in Software auf dem
Sitzungs-Thread (ohne D3D11-Manager); für 4K mit hoher Bildrate wäre DXVA der nächste Schritt.
Getestet mit einem im Test erzeugten H.264-Strom aus I_PCM-Makroblöcken (zwei IDR-Bilder, Werte exakt);
gegen einen echten Server ungetestet. macOS: VideoToolbox (`VTDecompressionSession`) nach demselben
Muster, noch offen.

**Lizenz und Patente (GPL-3.0-or-later):**

- Media Foundation und VideoToolbox gehören zum Betriebssystem. Die GPL erlaubt das Linken gegen
  Systembibliotheken (GPLv3 §1, «System Libraries»); die H.264-Patentlizenz hat Microsoft bzw. Apple mit
  dem System abgegolten. Sesam liefert keinen Codec mit. Das ist der bevorzugte Weg.
- **OpenH264** (Cisco) steht unter BSD-2-Clause und ist damit GPL-kompatibel. Die Patentlizenz (Via LA,
  früher MPEG LA) deckt Cisco aber **nur für die von Cisco gebauten und verteilten Binärdateien**, die
  zur Laufzeit von Cisco heruntergeladen werden (`ironrdp-egfx`, Feature `openh264-libloading`). Die
  Cisco-DLL darf nicht im Installer stecken, und Ciscos Hinweistext muss angezeigt werden. Selbst aus dem
  Quelltext gebaut und mitgeliefert (`openh264-bundled`) wäre der Code GPL-kompatibel, aber ohne
  Patentlizenz. Für Sesam darum höchstens als Rückfall auf Systemen ohne Decoder, nicht als Standard.
- FFmpeg (LGPL/GPL) wäre lizenzrechtlich möglich, bringt aber C-Bibliotheken in den Bau und dieselbe
  Patentfrage wie OpenH264.

### Sicherheit: was IronRDP und sspi können und was nicht

- **TLS:** rustls mit ring, TLS 1.2 und **TLS 1.3** (rustls-Standard), kein TLS 1.0/1.1, keine alten
  Chiffren, keine Sitzungswiederaufnahme. Hinweis: `ironrdp-tls` setzt `KeyLogFile`; ist die
  Umgebungsvariable `SSLKEYLOGFILE` gesetzt, landen die Sitzungsschlüssel in dieser Datei (wie bei
  Browsern, nur relevant, wenn jemand die Variable setzt).
- **Serverauthentisierung:** Zertifikatskette und Name gegen die Stammzertifikate des Systems; schlägt das
  fehl, entscheidet das Pinning (SHA256, beim ersten Mal bestätigt). Zusätzlich bindet CredSSP den
  öffentlichen Schlüssel des TLS-Zertifikats (pubKeyAuth), sodass ein Mann in der Mitte mit eigenem
  Zertifikat die NLA-Anmeldung nicht weiterreichen kann. Mit Kerberos authentisiert sich auch der Server
  gegenüber dem Client (gegenseitig über `TERMSRV/<host>`); bei NTLM nicht.
- **CredSSP-Version / CVE-2018-0886:** sspi spricht TSRequest Version 6 mit Client-Nonce und
  Hash-Bindung (ab v5). sspi selbst würde ungepatchte Server (v2 bis v4) noch bedienen; **Sesam lehnt
  Server unter Version 5 ab** (entspricht «Encryption Oracle Remediation: Mitigated» bei mstsc).
- **Restricted Admin:** nicht umgesetzt. sspi kennt `CredSspMode::CredentialLess`, aber
  `CredsspSequence::init` in ironrdp-connector 0.12 setzt fest `WithCredentials`, und der Connector setzt
  das X.224-Flag `RESTRICTED_ADMIN_MODE_REQUIRED` nicht. Machbar mit einem eigenen `CredSspClient` statt
  `CredsspSequence` und eigener X.224-Anfrage (wie bei RDSTLS); sinnvoll nur mit Kerberos und wenn der
  Server Restricted Admin erlaubt.
- **Remote Credential Guard:** mit sspi nicht machbar. Er braucht `TSRemoteGuardCreds` mit Anmeldedaten,
  die das lokale LSA (Kerberos-Paket von Windows) ausstellt; das geht nur über die native Windows-SSPI.
- **NTLM:** nur noch Rückfall und pro Verbindung abschaltbar (siehe oben). Passwörter bleiben aus dem Log
  (`diag::add_secret`), auch das Gateway-Passwort.

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
«Vergleichen» (`compare.ts`) markiert gleichnamige Einträge der beiden offenen Ordner nach Grösse und
Änderungszeit (± 2 s), nicht rekursiv; «Inhalt prüfen» rechnet SHA-256 lokal und auf dem Server mit
`sha256sum` (sonst `shasum -a 256`) über einen exec-Kanal derselben Verbindung. Ersetzen (`replace` bei
`transfer_start`, nur einzelne Dateien) gibt es nur nach ausdrücklichem «Ersetzen»; sonst «Name (2).ext».

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

## Titelleiste unter Windows: Andocklayouts (Snap Layouts)

Sesam zeichnet unter Windows eine eigene Titelleiste (ohne Fensterrahmen, `decorations: false`), damit
Tabs und Menü in der Leiste stehen. Windows 11 zeigt die Andocklayouts beim Darüberfahren über den
Maximieren-Knopf nur, wenn das Fenster bei `WM_NCHITTEST` für diesen Bereich `HTMAXBUTTON` meldet.
WebView2 liegt als eigenes Kindfenster über der ganzen Fläche und bekommt die Mausereignisse, das
Hauptfenster sieht den Treffertest dort nicht. Eine saubere Lösung braucht ein kleines natives
Kindfenster genau über dem Knopf, das `HTMAXBUTTON` meldet und Klicks/Hover an die Webansicht
weiterreicht (Ansatz von tauri-plugin-decorum). Das ist noch nicht umgesetzt, weil es nur am laufenden
Fenster prüfbar ist. Bis dahin: Andocklayouts mit **Win+Z** (steht im Tooltip des Knopfs), Ziehen an
den Bildschirmrand und Doppelklick auf die Leiste zum Maximieren funktionieren.
