# wayvnc (Wayland mit wlroots)

wayvnc läuft **nur auf wlroots-Desktops** wie Sway oder labwc. Für Ubuntu mit GNOME bitte
**RDP** nutzen.

## Installieren

```
sudo apt install wayvnc
```

## TLS und Anmeldung einrichten

Zertifikat und Schlüssel erzeugen:

```
mkdir -p ~/.config/wayvnc && cd ~/.config/wayvnc
```

```
openssl req -x509 -newkey ec -pkeyopt ec_paramgen_curve:secp384r1 -sha384 -days 3650 -nodes -keyout tls_key.pem -out tls_cert.pem -subj /CN=localhost
```

Datei `~/.config/wayvnc/config` anlegen (Benutzer und Passwort anpassen):

```
address=0.0.0.0
enable_auth=true
username=<benutzer>
password=<passwort>
private_key_file=tls_key.pem
certificate_file=tls_cert.pem
```

Starten (in der laufenden Wayland-Sitzung):

```
wayvnc
```

Port 5900 in der Firewall öffnen (bei aktivem ufw):

```
sudo ufw allow 5900/tcp
```

## In Sesam

- Desktop (VNC) aktiv, Port 5900.
- Benutzer und Passwort des Benutzers wie in der wayvnc-Konfiguration.
- Beim ersten Verbinden den Fingerabdruck des Zertifikats bestätigen.
