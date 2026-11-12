# Hinweise für App Review

## Das Problem

Sesam ist ein reiner Client: ohne einen Rechner mit RDP, VNC oder SSH zeigt die App nur die leere Bibliothek.
Einen Demo-Modus gibt es heute **nicht**. App Review lehnt solche Apps sonst mit Richtlinie 2.1 (App
Completeness, «we were unable to review your app as it requires access to a server») ab.

## Vorschlag (Entscheid Mike)

1. **Test-Server für die Prüfung (empfohlen).** Ein kleiner, öffentlich erreichbarer Server nur für App
   Review, z. B. eine VM bei Infomaniak:
   - SSH/SFTP mit eigenem Benutzer `review` (Passwort-Anmeldung, nur sein Home, kein sudo), ein paar
     Beispieldateien;
   - optional ein VNC- oder RDP-Desktop (z. B. Ubuntu mit GNOME Remote Desktop), damit auch das Remote-Bild
     geprüft werden kann;
   - nur während der Prüfung eingeschaltet, Passwort nach jeder Prüfung wechseln.
   Die Zugangsdaten stehen dann in App Store Connect › App-Prüfung › Anmeldeinformationen bzw. in den
   Notizen unten. **Nicht eingerichtet** (Auftrag: keinen öffentlichen Server aufsetzen).
2. **Video als Ergänzung.** Ein kurzes Bildschirmvideo vom iPad (Bibliothek → Shell → Dateien →
   Remotedesktop zu einem eigenen Rechner), als Link in den Notizen. Apple akzeptiert das für Apps, die
   eigene Hardware/Server brauchen; als alleiniger Nachweis reicht es nicht immer.
3. **Eingebaute Demo-Verbindung (nicht empfohlen).** Eine vorgetäuschte Shell oder Dateiliste ohne Server
   wäre Aufwand, zeigt nicht die echte Funktion und kann selbst als irreführend gelten. Eine «Beispiel-
   Verbindung» in der leeren Bibliothek, die auf den Test-Server aus 1. zeigt, wäre dagegen wenig Aufwand,
   bindet die App aber an einen Server, den es dauerhaft geben muss. Erst bauen, wenn Mike das will.

## Text für «Notizen» (App Store Connect › App-Prüfung), Englisch

```
Sesam Remote is a client for remote desktop (RDP, VNC), SSH shell and file transfer (SFTP/FTP). It connects
directly to computers the user enters; there is no account and no server operated by us.

To test, please add a connection to our review server:
1. Tap "Neue Verbindung" (New connection).
2. Name: Review, Host: <REVIEW_HOST>, user: review, password: <REVIEW_PASSWORD>.
   Under "Dienste" (Services): SSH port 22 is on; for the remote desktop also turn on VNC (port 5900) /
   RDP (port 3389).
3. Save, then tap the shell icon (terminal), the folder icon (files) or the screen icon (desktop) on the card.
4. On first connect, confirm the server fingerprint ("Vertrauen und verbinden").

The app's interface is German. iPadOS asks once for local network access; the review server is on the
internet, so this permission is not required for the test.

Background: iPadOS does not allow keeping SSH/RDP/VNC connections open in the background; Sesam reconnects
when the app returns to the foreground. This is expected behavior.

Encryption: standard algorithms only (SSH, TLS, AES), open source; export compliance answered accordingly.
A demo video is available at: <VIDEO_URL>
```

## Kontakt für App Review

- Vorname/Nachname: Mike Dörig
- E-Mail: support@sesam-remote.app (oder eine direkte Adresse, die Mike schnell liest)
- Telefon: Mikes Nummer (nicht im Repository)
- «Anmeldung erforderlich»: **Ja**, mit den Zugangsdaten des Test-Servers (Feld Benutzername/Passwort).

## Weitere Punkte, die App Review erfahrungsgemäss fragt

- **Hintergrund:** keine Hintergrundmodi in der Info.plist, also keine Rückfrage; der Text oben erklärt das
  Neuverbinden.
- **Lokales Netzwerk:** Text in `NSLocalNetworkUsageDescription` ist vorhanden.
- **Richtlinie 4.2 (Mindestfunktion):** erfüllt; native Funktionen (Schlüsselbund, Face ID, Kurzbefehl,
  Dateien-App, Drag & Drop, VideoToolbox).
- **Open Source/MPL-2.0:** App Store verträgt sich mit der MPL (`docs/app-store.md`).
