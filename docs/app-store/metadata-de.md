# App Store Connect: Texte (Deutsch)

Für die iPad-App, Version **26.11.3**. Hauptsprache in App Store Connect: Deutsch. Alles hier beschreibt nur
das, was der iPad-Build wirklich kann (`docs/plattformen.md`, Spalte iPad). Zeichenzahlen sind mit
`node -e` gezählt; Apple zählt Zeichen, nicht Bytes.

| Feld | Inhalt | Länge / Grenze |
|---|---|---|
| Name | Sesam Remote | 12 / 30 |
| Untertitel | Remotedesktop, SSH und Dateien | 30 / 30 |
| Kategorie | Primär: Dienstprogramme (Utilities). Sekundär: Entwickler-Tools (Developer Tools) | – |
| Preis | Gratis (wie auf sesam-remote.app), keine In-App-Käufe | – |
| Support-URL | https://sesam-remote.app/support.html | – |
| Marketing-URL | https://sesam-remote.app/ | – |
| Datenschutz-URL | https://sesam-remote.app/datenschutz.html | – |
| Copyright | © 2026 Mike Dörig | – |
| Version | 26.11.3 (Build 26.11.3) | – |

## Werbetext (170)

> Remotedesktop (RDP, VNC), SSH-Shell und Dateien (SFTP, FTP) in einer App. Ohne Konto und ohne Cloud: Sesam verbindet sich direkt mit deinen Rechnern.

## Beschreibung (4000)

```
Sesam Remote bringt Desktop, Shell und Dateien deiner Rechner auf das iPad, in einer App und mit einer Bibliothek für alle Verbindungen.

REMOTEDESKTOP
• RDP für Windows (Pro/Enterprise), Windows Server und Linux (GNOME Remote Desktop, xrdp), mit Ton und gemeinsamer Zwischenablage für Text
• VNC für die Bildschirmfreigabe des Mac, Raspberry Pi und Linux
• Touch-Bedienung wie ein Touchpad oder direkt, Zoom, Bildschirmtastatur mit Zusatzleiste für Strg, Alt, Esc, Tab, Pfeile und F-Tasten, «Tasten senden» für Tastenkombinationen
• H.264 im Remote-Bild über die Hardware des iPad

SHELL (SSH)
• Terminal mit Tabs, Suche, Textbausteinen und vielen Farbschemata, darunter eigene Sesam-Designs in Hell und Dunkel
• Anmeldung mit Passwort oder SSH-Schlüssel; Schlüssel erzeugt Sesam auf Wunsch selbst
• Gehaltene Shell mit tmux: Die Sitzung läuft auf dem Server weiter, auch wenn iPadOS die Verbindung im Hintergrund trennt
• Türöffner: Dienste im Netz des Servers über einen SSH-Tunnel erreichen

DATEIEN (SFTP, FTP)
• Zwei Seiten nebeneinander: links das iPad (Ordner aus der App «Dateien», iCloud Drive, USB-Stick, Freigaben), rechts der Server
• Hoch- und herunterladen, umbenennen, löschen mit Papierkorb auf dem Server und Rückgängig
• Dateien aus der App «Dateien» direkt auf Shell oder Remote-Bild ziehen (Split View, Slide Over)

BIBLIOTHEK
• Karten oder Liste, Gruppen mit Farben, Suche
• Online-Status und Aufwecken per Wake-on-LAN
• «Erste Hilfe» prüft Namensauflösung, Erreichbarkeit, Port, Dienst und Anmeldung und zeigt, wo es hakt
• «Desktop freischalten» richtet per SSH die Bildschirmfreigabe auf Ubuntu und Raspberry Pi ein, Schritt für Schritt und mit Rückfrage
• Kurzbefehl «Verbindung öffnen» für die App «Kurzbefehle», Spotlight und Siri

SICHERHEIT UND DATENSCHUTZ
• Passwörter und Schlüssel liegen im Schlüsselbund des iPad; Anzeigen nur nach Face ID, Touch ID oder Gerätecode
• Fingerabdrücke von Servern und Zertifikaten werden geprüft und gemerkt
• Kein Konto, kein Vermittlungsserver, keine Werbung, kein Tracking: Sesam sendet keine Daten an uns
• Verschlüsselte Sicherung von Bibliothek und Zugangsdaten als Datei

GUT ZU WISSEN
• Sesam ist ein Client. Die Rechner, mit denen du dich verbindest, brauchen eingeschaltetes RDP, VNC oder SSH.
• iPadOS hält Verbindungen im Hintergrund nicht offen. Wechselst du die App, verbindet Sesam beim Zurückkommen neu; eine gehaltene Shell (tmux) läuft auf dem Server weiter.
• Systemkürzel wie ⌘Tab bleiben beim iPad; dafür gibt es die Zusatzleiste und «Tasten senden».
• Für Rechner im eigenen Netz fragt iPadOS einmal nach dem Zugriff auf das lokale Netzwerk.

Sesam Remote ist Open Source (Mozilla Public License 2.0) und gibt es auch für Mac, Windows und Linux auf sesam-remote.app.
```

## Schlüsselwörter (100)

```
SSH,RDP,VNC,SFTP,FTP,Remotedesktop,Terminal,Fernzugriff,Server,Raspberry Pi,Linux,Windows,tmux,Shell
```

Ohne Leerzeichen nach den Kommas; «Sesam» und «Remote» stehen schon im Namen und zählen dort.

## Neu in dieser Version

```
Erste Version im App Store.
```

## Bewusst nicht behauptet

- Keine Ordnerfreigabe über RDP, kein lokales Terminal, keine eigenen Fenster, keine Dateien/Bilder in der
  gemeinsamen Zwischenablage (iPad: nur Text), kein Widget (braucht die App Group, `docs/plattformen.md`).
- RDP und Hochladen über die Dateiauswahl sind auf dem echten iPad noch nicht geprüft (`docs/app-store.md`);
  vor dem Einreichen einmal mit einem Windows-PC testen, sonst die RDP-Zeilen abschwächen.
