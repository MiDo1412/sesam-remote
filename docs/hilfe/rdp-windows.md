# RDP mit Windows

Als Ziel für Remotedesktop eignen sich nur **Windows Pro und Enterprise**; Windows Home bietet
keinen Remotedesktop-Zugang.

## Einrichten

**Einstellungen › System › Remotedesktop** einschalten. Windows öffnet dabei die Firewall für
Port 3389.

Standardmässig dürfen sich Administratoren anmelden. Weitere Konten unter **Remotedesktopbenutzer**
hinzufügen.

## In Sesam

- RDP aktiv, Port 3389.
- Benutzer: das Windows-Konto. Bei einem Microsoft-Konto die E-Mail-Adresse.
- Domäne: nur in einer Firmendomäne, sonst leer lassen.
- Beim ersten Verbinden zeigt Sesam den Fingerabdruck des Zertifikats zum Bestätigen.
