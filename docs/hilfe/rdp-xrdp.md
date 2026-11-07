# RDP mit xrdp (Linux)

xrdp ist der verbreitete RDP-Server für Linux ohne GNOME Remote Desktop, zum Beispiel für Debian,
Fedora, Linux Mint, Raspberry Pi OS oder Desktops wie Xfce und KDE.

## Einrichten

Auf dem Zielrechner (Beispiel Debian/Ubuntu-Familie):

```
sudo apt install xrdp
sudo systemctl enable --now xrdp
```

Bei Fedora: `sudo dnf install xrdp` und danach dasselbe `systemctl`-Kommando. Ist eine Firewall aktiv,
Port 3389 freigeben.

## In Sesam

- **Was möchtest du öffnen?** «Windows-PC oder anderer RDP-Server»
- **Port:** 3389
- **Anmeldung:** Benutzer und Passwort des Linux-Kontos. Ein eigenes RDP-Passwort gibt es bei xrdp nicht.

## Wenn es nicht klappt

- **Schwarzer Bildschirm nach der Anmeldung:** Meist ist der Benutzer gleichzeitig direkt am Rechner
  angemeldet. xrdp startet eine eigene Sitzung; am Rechner abmelden und neu verbinden.
- **Zertifikat unbekannt:** xrdp nutzt ein selbst erzeugtes Zertifikat. Den Fingerabdruck einmal prüfen
  und vertrauen; Sesam merkt ihn sich.
