# Welche Zugangsdaten brauche ich?

Sesam fragt nur nach dem, was der Server wirklich verlangt. Felder, die du nicht brauchst, bleiben leer.

| Feld | Wofür | Wann ausfüllen |
|---|---|---|
| **Benutzer** und **Passwort** | Anmeldung am Konto: SSH, SFTP, macOS-Bildschirmfreigabe, VNC mit Benutzername (wayvnc), RDP ohne eigenes RDP-Passwort | Fast immer |
| **RDP-Passwort** | Nur wenn RDP ein *anderes* Passwort hat als das Konto, z. B. die frei gewählten Zugangsdaten der GNOME-Freigabe | Nur bei GNOME-Freigabe oder Remote-Anmeldung |
| **VNC-Passwort** | Das eigene Passwort eines klassischen VNC-Servers (ohne Benutzername) | Nur bei klassischem VNC |
| **SSH-Schlüssel** | Anmeldung per Schlüssel statt Passwort | Optional, nur SSH/SFTP |

Ist das RDP-Passwort leer, nimmt Sesam für RDP das normale Passwort.

## GNOME (Ubuntu) im Überblick

- **Remote-Anmeldung** (Port 3389): Benutzer und RDP-Passwort sind die Zugangsdaten, die in
  *Einstellungen › System › Remote-Desktop › Remote-Anmeldung* stehen. Danach meldest du dich im
  Anmeldebildschirm mit dem normalen Konto-Passwort an; Sesam übernimmt die Umleitung selbst.
- **Bildschirmfreigabe** (Port 3390 oder wie angezeigt): Benutzer und RDP-Passwort aus
  *Einstellungen › System › Remote-Desktop › Desktopfreigabe* des angemeldeten Benutzers.

## Gespeicherte Zugangsdaten

Nutzen mehrere Verbindungen dasselbe Konto, lege es einmal unter *Einstellungen › Zugangsdaten* an
und wähle es im Verbindungs-Formular unter **Zugangsdaten** aus. Ändert sich das Passwort, passt du
es nur an einer Stelle an. Eigene Zugangsdaten einer Verbindung übernimmst du mit
**Als gespeicherte Zugangsdaten sichern …**.

Gespeicherte Zugangsdaten gelten für Benutzer, Passwort und SSH-Schlüssel. RDP- und VNC-Passwort
bleiben bei der Verbindung.

Alle Passwörter und Schlüssel liegen im Schlüsselbund des Betriebssystems (Windows-Anmeldeinformationen,
macOS-Schlüsselbund), nie in Klartext-Dateien und nie im Protokoll.
