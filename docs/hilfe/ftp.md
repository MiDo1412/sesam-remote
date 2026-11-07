# FTP und FTPS

Für Webserver und Hosting-Angebote ohne SSH: im Sheet unter «Dienste» **Dateien über FTP**
einschalten. Benutzername und Passwort sind die der Verbindung (oder gespeicherte Zugangsdaten).
Hat die Verbindung auch SSH, wählst du bei «Dateien öffnen über», was die Ansicht «Dateien» nimmt.

## FTPS oder FTP?

- **FTPS (explizit, TLS)** ist die richtige Wahl für fast alle Anbieter: Sesam verbindet auf Port 21
  und schaltet mit `AUTH TLS` auf Verschlüsselung um, bevor Passwort und Dateien fliessen.
- **FTPS (implizit)** ist die ältere Form: verschlüsselt ab dem ersten Byte, meist auf Port 990.
  Nur wählen, wenn der Anbieter das so angibt.
- **Unverschlüsselt** schickt Passwort und Dateien im Klartext. Wer im selben WLAN oder unterwegs
  mithört, kann beides lesen. Nur nehmen, wenn der Server wirklich kein FTPS kann, und das
  Passwort danach nirgends sonst verwenden.

Meldet Sesam «Der Server bietet kein FTPS an», kann der Server kein `AUTH TLS`. Frag zuerst
beim Anbieter nach FTPS oder SFTP, bevor du auf unverschlüsselt wechselst.

## Zertifikat

Beim ersten Verbinden zeigt Sesam den Fingerabdruck des Zertifikats, wie bei RDP. Vergleiche ihn mit
den Angaben des Anbieters. Sesam merkt sich das Zertifikat (Einstellungen › Sicherheit › Gemerkte Zertifikate).
Erneuert der Anbieter es (bei Let's Encrypt alle paar Wochen), fragt Sesam noch einmal.

## Passiv oder aktiv?

**Passiv** (Standard) klappt fast immer: Sesam öffnet auch die Datenverbindung selbst.
**Aktiv** heisst, der Server verbindet zurück zu diesem Computer; das scheitert meist an Router und
Firewall und ist nur für alte Server im eigenen Netz gedacht.

## Was geht

Auflisten, Hoch- und Herunterladen (auch ganze Ordner), Neuer Ordner, Neue Datei, Umbenennen und
Löschen. «Rechte ändern» schickt `SITE CHMOD`, das nicht jeder Server kann. Wie bei SFTP wird nichts
überschrieben: ein belegter Name wird zu «Name (2).ext». Jede Übertragung nutzt eine eigene
Verbindung; erlaubt der Anbieter nur wenige gleichzeitige Verbindungen, Übertragungen nacheinander
starten.
