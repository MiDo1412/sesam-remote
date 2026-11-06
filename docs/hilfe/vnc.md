# VNC allgemein

VNC überträgt den Bildschirm eines Rechners. Welcher VNC-Server passt, hängt vom Desktop ab:

| Zielsystem | VNC-Server |
|---|---|
| Ubuntu mit GNOME (Wayland) | kein VNC mehr, bitte **RDP** nutzen |
| Wayland mit wlroots (Sway, labwc) | **wayvnc** |
| Linux mit X11 | **x11vnc** |
| macOS | **Bildschirmfreigabe** (eingebaut) |

## Sicherheit

Klassisches VNC verschlüsselt die Übertragung nicht. Ausserhalb des eigenen Netzes nur mit
**TLS (VeNCrypt)** verbinden, z. B. wayvnc mit Zertifikat. Sesam lehnt unverschlüsselte
Anmeldung mit Klartext-Passwort ab.

## In Sesam

- Desktop (VNC) aktiv, Port meist 5900.
- **VNC-Passwort:** nur nötig, wenn der Server ein eigenes VNC-Passwort hat.
- Bei TLS zeigt Sesam beim ersten Verbinden den Fingerabdruck des Zertifikats zum Bestätigen.
- Port 5900 in der Firewall des Zielrechners öffnen, falls eine aktiv ist.
