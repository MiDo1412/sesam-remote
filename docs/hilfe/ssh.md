# SSH und SFTP

SSH bietet die Shell, SFTP den Dateitransfer (Ansicht «Dateien») und die Übergabe von Dateien und
Bildern über die gemeinsame Zwischenablage.

## Linux

```
sudo apt install openssh-server
```

```
sudo systemctl enable --now ssh
```

Port 22 in der Firewall öffnen (bei aktivem ufw):

```
sudo ufw allow 22/tcp
```

## macOS

**Systemeinstellungen › Allgemein › Teilen › Entfernte Anmeldung** einschalten.

## Mit Schlüssel statt Passwort anmelden

Sesam nimmt **ed25519- und ECDSA-Schlüssel**, im OpenSSH-Format oder als PuTTY-Datei (`.ppk`, Version 2 und 3).
RSA passt nicht in den Tresor von Windows. Ist der Schlüssel mit einer Passphrase geschützt, gilt das
Passwort der Verbindung als Passphrase. Auf diesem Computer erzeugen:

```
ssh-keygen -t ed25519
```

Den öffentlichen Schlüssel auf den Server bringen:

```
ssh-copy-id <benutzer>@<server>
```

Unter Windows ohne `ssh-copy-id` den Inhalt von `%USERPROFILE%\.ssh\id_ed25519.pub` auf dem
Server ans Ende von `~/.ssh/authorized_keys` anfügen.

**In Sesam:** im Sheet bei «SSH-Schlüssel» «Schlüssel importieren …» wählen und den Inhalt von
`id_ed25519` (ohne `.pub`) oder der `.ppk`-Datei einfügen. Der Schlüssel liegt danach im Tresor.

## Beim ersten Verbinden

Sesam zeigt den Fingerabdruck des Servers. Vergleichen mit:

```
ssh-keygen -lf /etc/ssh/ssh_host_ed25519_key.pub
```

## Erweitert

Im Sheet unter «Erweitert»:

- **Dateiübertragung:** Automatisch nimmt SFTP und weicht auf SCP aus, wenn der Server kein SFTP hat.
  Mit SCP geht Hoch- und Herunterladen und «Neuer Ordner», aber kein Umbenennen und Löschen.
- **Sprungserver:** eine andere Verbindung aus der Bibliothek, über die Sesam weiterverbindet
  (wie `ssh -J`). Den Sprungserver vorher einmal direkt öffnen und seinen Fingerabdruck bestätigen.
- **SSH-Agent:** nimmt die Schlüssel aus Pageant oder dem OpenSSH-Agenten von Windows.
  «Agent weiterreichen» nur bei Servern, denen du vertraust.
- **Portweiterleitungen:** je Zeile eine Regel, aktiv solange eine Shell offen ist.
  `L8080 localhost:80` macht Port 80 des Servers hier unter `localhost:8080` erreichbar,
  `R9000 localhost:3000` macht Port 3000 dieses Computers auf dem Server unter Port 9000 erreichbar,
  `D1080` ist ein SOCKS5-Proxy auf `localhost:1080`.
- **Keepalive, Kompression, veraltete Algorithmen:** für instabile Leitungen und alte Server.

## Shell auf dem Server halten (tmux)

Unter *Bearbeiten › Dienste › Shell auf dem Server halten* läuft die Shell in einer tmux-Sitzung auf dem Server. Bricht die Verbindung ab (Ruhezustand, WLAN-Wechsel, VPN), läuft dort alles weiter; Sesam verbindet neu und hängt dieselbe Sitzung wieder an. Jeder Shell-Tab einer Verbindung hat seine eigene Sitzung (`sesam-…-0`, `-1` …).

Sesam stellt tmux so ein, dass es sich wie ein normales Terminal anfühlt: keine Statuszeile, keine tmux-Tastenkürzel, das Mausrad blättert im Verlauf. Markierst du Text mit der Maus, landet er beim Loslassen in der Zwischenablage dieses Rechners (tmux schickt ihn per OSC 52). Ganz normal wie ohne tmux markierst du mit gedrückter Wahltaste (⌥, Mac) bzw. Umschalttaste (Windows). Bei sehr altem tmux (vor 2.2) bleiben einzelne dieser Einstellungen aus.

Fehlt tmux auf dem Server, startet eine normale Shell mit einem Hinweis; Sesam installiert nichts. Unter *… › Gehaltene Sitzungen* siehst du die Sesam-Sitzungen auf diesem Server, auch verwaiste, und kannst sie beenden.

Nach dem ersten Abbruch einer Shell fragt Sesam einmal, ob es sie künftig halten soll.

## Papierkorb auf dem Server und Rückgängig

Was du im Dateifenster löschst oder beim Hochladen ersetzt, löscht Sesam nicht sofort: es kommt in einen versteckten Papierkorb im Home des Benutzers auf dem Server (`~/.sesam-papierkorb/<Datum-Zeit>/`). In der Übertragungsliste steht dazu **Rückgängig**; ⌘Z (Mac) bzw. Strg+Z holt das zuletzt Gelöschte zurück. Nach 7 Tagen (einstellbar unter *Einstellungen › Allgemein*) räumt Sesam beim nächsten Verbinden auf.

Liegt etwas auf einem anderen Dateisystem als das Home (z. B. ein eingehängtes Laufwerk), lässt es sich nicht in den Papierkorb verschieben. Dann fragt Sesam, ob es endgültig löschen soll. Mit Administratorrechten (sudo) Gelöschtes kommt ebenfalls in den Papierkorb, gehört dort aber root; zurückholen geht dann nur mit sudo.

Bei FTP gibt es keinen Papierkorb; dort löscht Sesam nach Rückfrage endgültig.
