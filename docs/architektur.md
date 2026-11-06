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

## Plattformen

Zuerst Windows. macOS ist ohne Code-Änderung zu erwarten. iPad (`tauri ios init`) braucht
macOS mit Xcode und den iOS-Tresor (siehe vault).
