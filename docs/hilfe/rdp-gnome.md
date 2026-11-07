# RDP mit GNOME Remote Desktop (Ubuntu)

Ab Ubuntu 26.04 mit GNOME 50 läuft der Desktop nur noch unter Wayland. GNOME Remote Desktop bietet
dort **nur RDP**, kein VNC mehr. Es gibt zwei Arten:

- **Remote Login:** Sesam verbindet sich mit dem Anmeldebildschirm. **Jeder Benutzer des Rechners**
  meldet sich dort mit seinem eigenen Konto an, wie am Rechner selbst. Danach übergibt GNOME die
  Sitzung; Sesam folgt dieser Übergabe automatisch.
- **Bildschirmfreigabe:** Sesam zeigt eine bereits laufende, angemeldete Sitzung.

## Am einfachsten: Setup-Script

Das Setup-Script von Sesam richtet Anmeldebildschirm (Port 3389), Bildschirmfreigabe pro Benutzer,
Zertifikate, Firewall und auf Wunsch einen virtuellen Monitor ein. Es fragt jeden Schritt nach und
zeigt am Ende Ports und Zertifikats-Fingerabdrücke zum Vergleichen.

## Remote Login für alle Benutzer einrichten

Die Zugangsdaten des Systemdienstes sind **nur für die Verbindung** da. Die eigentliche Anmeldung
macht danach jede Person am Anmeldebildschirm mit ihrem eigenen Konto.

Remote Login einschalten:

```
sudo grdctl --system rdp enable
```

Zugangsdaten für die Verbindung setzen. grdctl fragt Benutzername und Passwort selbst ab; nicht als
Argumente angeben, sonst landet das Passwort im Systemprotokoll:

```
sudo grdctl --system rdp set-credentials
```

TLS-Zertifikat und Schlüssel setzen:

```
sudo grdctl --system rdp set-tls-cert /pfad/zum/zertifikat.pem
```

```
sudo grdctl --system rdp set-tls-key /pfad/zum/schluessel.pem
```

Dienst starten und beim Systemstart aktivieren:

```
sudo systemctl enable --now gnome-remote-desktop.service
```

Port 3389 in der Firewall öffnen (bei aktivem ufw):

```
sudo ufw allow 3389/tcp
```

Stand prüfen:

```
sudo grdctl --system status
```

**In Sesam:** RDP aktiv, Port 3389. Als Benutzer und RDP-Passwort die Zugangsdaten aus
`set-credentials` eintragen. Beim ersten Verbinden zeigt Sesam den Fingerabdruck des Zertifikats
zum Bestätigen. Danach erscheint der GNOME-Anmeldebildschirm; dort mit dem eigenen Konto anmelden.

## Bildschirmfreigabe der laufenden Sitzung

In der Sitzung, die freigegeben werden soll:
**Einstellungen › System › Remotedesktop › Desktopfreigabe** einschalten und Benutzername und
Passwort für die Freigabe festlegen.

Oder im Terminal derselben Sitzung:

```
grdctl rdp enable
```

```
grdctl rdp set-credentials
```

**In Sesam:** RDP aktiv, Port wie in den Einstellungen angezeigt, Benutzer und RDP-Passwort aus der
Freigabe. Läuft Remote Login gleichzeitig, nutzt die Freigabe einen anderen Port als 3389.

## Rechner ohne Monitor

Ohne angeschlossenen Monitor gibt es keinen Bildschirm, den GNOME freigeben kann. Abhilfe ist ein
erzwungener Monitor per Kernel-Parameter (Anschluss und EDID-Datei an den Rechner anpassen):

```
drm.edid_firmware=HDMI-A-1:edid/monitor.bin video=HDMI-A-1:e
```
