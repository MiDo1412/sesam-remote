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
