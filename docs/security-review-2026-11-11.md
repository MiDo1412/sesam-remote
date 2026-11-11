# Sicherheitsprüfung 11.11.2026

Prüfung von Sesam Remote vor der ersten Version 26.11.1, mit dem Audit-Skill von Cloudflare
(`security-audit-skill`, bei alldesk als `ad-sec-audit`). Geprüft wurde der Quelltext; ausgeführt wurden nur
eigene Unit-Tests auf diesem Rechner (keine Server im Netz, keine fremden Dienste).

- **Stand:** Commit `d27b9cd` plus die Korrekturen aus dieser Prüfung (im selben Release).
- **Vorgehen:** sechs unabhängige Prüfbereiche (Tauri-Befehle und Dateipfade; Zugangsdaten und Protokolle;
  TLS, Host-Schlüssel und Pinning; Deep Links, Webview und Zwischenablage; Dateioperationen und
  Protokoll-Parser; Lieferkette und Release). Jeder Befund wurde danach von einer eigenen, unabhängigen Prüfung
  gegen den alten und den korrigierten Quelltext bestätigt.
- **Ergebnisdateien:** `findings.json` (geprüft mit dem Schema des Skills, «PASS: 12 findings valid»)
  ausserhalb des Repositorys unter `~/security-audit-skill/sesam-remote/run-1/`.
- **Grenzen:** Das Abdeckungs-Ledger des Skills liegt nicht im maschinenprüfbaren Format vor; die Abdeckung steht
  unten in Worten. Windows- und Linux-Teile wurden nur im Code geprüft. `cargo audit` ist nicht installiert;
  die Rust-Abhängigkeiten sind von Hand gegen bekannte Hinweise abgeglichen worden. `npm audit` meldet 0.

## Übersicht

| Nr. | Befund | Schwere | Stand |
|---|---|---|---|
| S-01 | RD-Gateway: TLS-Zertifikat wurde nicht geprüft, Gateway-Passwort an jeden im Netz | **hoch** | behoben |
| S-02 | KDC-Proxy über HTTPS ohne Zertifikatsprüfung | mittel | behoben |
| S-03 | VNC: Rückfall von verschlüsselter auf unverschlüsselte Anmeldung trotz gemerktem Zertifikat | mittel | behoben (Vertrauen beim ersten Kontakt bleibt) |
| S-04 | Befehlsausführung als root über einen Dateinamen im sudo-Skript beim Hochladen | mittel | behoben, mit Test |
| S-05 | «Bearbeiten» führte ausführbare Dateien vom Server lokal aus | mittel | behoben (Sperrliste je System) |
| S-08 | Fremde Sicherung konnte vorhandene Zugangsdaten nutzen oder überschreiben | mittel | behoben, mit Test |
| S-09 | SSH-Schlüssel anderen Typs sah aus wie ein neuer Server | mittel | entschärft (deutliche Warnung) |
| S-06 | VNC-Zwischenablage holte beliebige Dateien und Ordner ungefragt | niedrig | entschärft (Grenzen) |
| S-07 | Sicherung mit allen Passwörtern ohne Touch ID | niedrig | behoben auf Mac/iPad; Windows/Linux offen |
| S-10 | Server setzte die lokale Zwischenablage über das Terminal (OSC 52) ohne Bedingung | niedrig | behoben |
| S-11 | VNC: unbegrenzte Bildgrösse (Speicher) | niedrig | behoben |
| S-12 | Installer ohne vertrauenswürdige Signatur | – (braucht Klärung) | offen, Zertifikate fehlen |

Kein Befund der Stufe «kritisch». Kein Weg von Server-Daten in die Webview als Skript (XSS) gefunden: Server-Daten
gehen über `textContent`, die CSP erlaubt keine fremden Skripte.

## Befunde

### S-01 RD-Gateway ohne Zertifikatsprüfung (hoch, behoben)

`rdp.rs` › `transport()` rief `GwClient::connect_with_port` auf. Diese Funktion nutzt in ironrdp-mstsgu 0.1 die
Voreinstellung von ironrdp-tls 0.3, und die heisst `DangerouslyAcceptInvalidCertificate`. Der Kommentar im Code
behauptete eine strikte Prüfung. Ein Angreifer im Netz (öffentliches WLAN) konnte sich als Gateway ausgeben und
bekam Gateway-Benutzer und -Passwort per HTTP-Basic im Klartext oder eine NTLM-Antwort; meist sind das
Domänen-Zugangsdaten.

**Korrektur:** `connect_with_port_and_certificate_validation(…, CertificateValidation::Strict, None)`, Prüfung
gegen die Stammzertifikate des Systems wie bei mstsc.

### S-02 KDC-Proxy ohne Zertifikatsprüfung (mittel, behoben)

`kdc.rs` nutzte `ironrdp_tls::upgrade` mit derselben unsicheren Voreinstellung. Ein Angreifer sah die
Kerberos-Anfragen (Offline-Angriff aufs Passwort) und konnte auf NTLM zurückzwingen. **Korrektur:**
`upgrade_with_certificate_validation(…, Strict)`.

### S-03 VNC-Downgrade (mittel, behoben)

Die Liste der VNC-Anmeldeverfahren kommt ungeschützt. Strich ein Angreifer VeNCrypt, wählte Sesam Apple-Auth
(unauthentisiertes Diffie-Hellman) oder VNC-Auth und gab das Kontopasswort preis, auch wenn für den Server
schon ein Zertifikat gemerkt war. **Korrektur:** Ist ein Zertifikat gemerkt, nimmt Sesam nur noch VeNCrypt
(oder «ohne Anmeldung», dann geht kein Passwort hinaus) und meldet sonst einen möglichen Angriff. Die
Apple-DH-Werte werden auf Mindestlänge und schwache Werte geprüft. Test: `kein_rueckfall_nach_gemerktem_zertifikat`.
**Rest:** Vor dem ersten verschlüsselten Kontakt gilt Vertrauen beim ersten Mal (wie bei SSH).

### S-04 Befehlsausführung als root über Dateinamen (mittel, behoben)

Beim Hochladen mit Administratorrechten (`sftp.rs` › `move_script`, ebenso `sudo_op_script` für Umbenennen
und Anlegen) stand der Dateiname roh in `echo "…«name»…"`; `$(…)` oder Backticks im Namen liefen mit
`sudo sh -c` als root, sobald die Zieldatei schon existierte. **Korrektur:** Meldung mit `scp::quote` und
`printf '%s\n'`. Test `dateiname_laeuft_nie_als_shell_code` führt die drei Skripte lokal mit einem bösartigen
Namen aus und prüft, dass nichts davon läuft.

### S-05 «Bearbeiten» führte Dateien aus (mittel, behoben)

`local_open` öffnete jede Datei vom Server mit der Standardaktion des Systems. Unter Windows starten so `.bat`,
`.js`, `.hta`, `.lnk` usw., auf dem Mac `.command`, `.terminal`, `.fileloc`. **Korrektur:** Sperrliste je
System (`RUNS_ON_OPEN`), dann eine klare Meldung. Test `ausfuehrbares_wird_nicht_geoeffnet`. **Rest:** eine
Sperrliste ist nie vollständig; Quarantäne bzw. Mark-of-the-Web setzt Sesam noch nicht.

### S-08 Fremde Sicherung (mittel, behoben)

Eine eingelesene `.sesam`-Datei konnte neue Verbindungen auf vorhandene Zugangsdaten zeigen lassen (die gingen
dann an den Host aus der Datei), über die Kennung «cred-…» Geheimnisse vorhandener Zugangsdaten überschreiben
und Agent-Weiterleitung oder einen vorhandenen Sprungserver mitbringen. **Korrektur:** Verweise nur auf
Zugangsdaten und Sprungserver aus derselben Sicherung, «cred-»-Kennungen abgelehnt, vorhandene Geheimnisse nie
überschrieben, Agent-Weiterleitung aus. Die Meldung nennt die entfernten Zuweisungen. Test
`fremde_zuweisungen_und_reservierte_kennungen`. **Rest:** Portweiterleitungen und Ordnerfreigabe kommen weiter
mit; die Vorschau zeigt sie noch nicht.

### S-09 SSH-Schlüssel anderen Typs (mittel, entschärft)

russh meldet einen bekannten Host mit Schlüssel anderen Typs als «nicht bekannt»; Sesam zeigte die gleiche
Rückfrage wie beim ersten Kontakt. **Korrektur:** Die Rückfrage heisst jetzt «Anderer Server-Schlüssel» und warnt
deutlich. Kein harter Abbruch, weil ältere `known_hosts` oft nur RSA/ECDSA kennen und der Client einen anderen
Typ bevorzugt. **Nächster Schritt:** Host-Key-Algorithmen auf die bekannten Typen einschränken (wie OpenSSH).

### S-06 VNC-Zwischenablage (niedrig, entschärft)

Eine `file://`-Liste vom VNC-Server löste ein rekursives Herunterladen ohne Grenze aus. **Korrektur:** höchstens
100 reguläre Dateien und 512 MB, keine Ordner. **Rest:** geprüft wird vor dem Herunterladen, nicht währenddessen.

### S-07 Sicherung ohne Bestätigung (niedrig, teilweise)

Die Sicherung enthält alle Passwörter, verlangte aber keine Bestätigung wie «Passwort anzeigen». **Korrektur:**
Touch ID bzw. Face ID auf Mac und iPad. **Offen:** Windows und Linux, bis Windows Hello eingebaut ist.

### S-10 OSC 52 (niedrig, behoben)

Terminalausgabe konnte die lokale Zwischenablage jederzeit ersetzen (z. B. mit einem Befehl, der später lokal
eingefügt wird). **Korrektur:** nur mit «Gemeinsame Zwischenablage», bei aktivem Tab und solange Sesam vorne ist.

### S-11 VNC-Bildgrösse (niedrig, behoben)

Bis 65535 × 65535 Pixel (17 GB). **Korrektur:** je Seite höchstens 16384, Fläche höchstens 8192².

### S-12 Signatur der Installer (braucht Klärung)

macOS-Builds sind nur ad hoc signiert und nicht notarisiert, Windows-Installer unsigniert. Es gibt keinen
Updater, also keinen angreifbaren Update-Kanal. Zu jedem Release gehört `SHA256SUMS.txt`. **Nächster Schritt:**
Developer ID mit Notarisierung, Authenticode für Windows.

## Nachtrag: automatische Updates (neu, vor dem Release geprüft)

Eingebaut nach der Prüfung (`src-tauri/src/update.rs`, offizielles `tauri-plugin-updater`). Geprüft:

- **Echtheit:** Jedes Update ist mit ed25519 signiert; die App enthält nur den öffentlichen Schlüssel.
  `requireSignedVersion` bindet die Version an die Signatur. Eine manipulierte `latest.json` kann also kein
  älteres, echtes Paket als neueres ausgeben. Nur neuere Versionen. Getestet: falsche Version abgelehnt, ältere
  nicht installiert.
- **Transport:** nur HTTPS (`dangerousInsecureTransportProtocol` nur in einer Test-Konfiguration).
- **Angriffsfläche Webview:** Die Webview hat keine Rechte für das Updater-Plugin, nur eigene Befehle (Stand,
  Schalter, Suchen, Installieren). Einen Ort, eine URL oder einen Schlüssel kann sie nicht vorgeben.
- **Windows:** Die Richtlinie liest `reg.exe` mit vollem Pfad (sonst hätte ein `reg.exe` neben einer portablen App
  Vorrang). MSI-Installationen installieren nie still mit Administratorrechten.
- **Rest:** Wer den privaten Schlüssel hat, kann Updates ausliefern. Er liegt deshalb ausserhalb des Repositorys,
  mit Passwort, und braucht eine Sicherung (`docs/release.md`). `latest.json` selbst ist nicht signiert; mehr als
  ausbleibende Updates erreicht ein Angreifer damit nicht.

## Weitere Beobachtungen

- **iPad und Stammzertifikate:** `rustls-native-certs` liest auf iOS nur OpenSSL-Pfade und findet dort
  wahrscheinlich keine Stammzertifikate (aus dem Quelltext, auf dem Gerät nicht geprüft). Dann scheitern auf dem
  iPad die strikte Prüfung des RD-Gateways und des KDC-Proxys und auch der direkte RDP-Aufbau, der denselben
  Weg nutzt. Vorher lief das Gateway dort ohne Prüfung. Abhilfe: Stammzertifikate über `webpki-roots` oder den
  Verifier des Systems (`rustls-platform-verifier`). Beides ist neu im Abhängigkeitsbaum und braucht ein OK.
- **Geringe Wirkung, offen:** `sesam://open?id=…` öffnet eine gespeicherte Verbindung ohne Rückfrage. Die IDs sind
  zufällig, und Ziel ist immer ein eigener Host. Rekursives Herunterladen hat keine Tiefen- oder Mengengrenze
  (Speicher des eigenen Clients). Tight-JPEG auf dem Mac geht an den Bilddecoder des Systems, bevor die Grösse
  geprüft ist.
- **Linux-Abhängigkeiten:** gtk-rs 0.18 (über Tauri/wry) hat Hinweise auf fehlende Wartung und RUSTSEC-2024-0429.
  Sesam ruft die betroffene Funktion nicht auf; mit Tauri-Updates mitziehen.
- **Ohne Befund:** Sicherungsformat (AES-256-GCM, Argon2id, frischer Salt und Nonce). sudo-Passwort nur über
  stdin. Diagnose-Log maskiert Geheimnisse. `SESAM_DIAG_JS` nur in Debug-Builds. Pfad-Traversal beim
  Herunterladen ist durch `check_name` abgewehrt. SSH-Sprungserver prüft das Ziel. FTPS mit Pinning und ohne
  Klartext-Rückfall. RDP direkt mit strikter Prüfung und Pinning. Capabilities schlank, CSP ohne fremde Skripte.
  Keine Geheimnisse im Repository oder in der Git-Historie (nur markierte Test-Schlüssel).
