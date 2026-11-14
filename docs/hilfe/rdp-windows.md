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

## Nur PIN oder kennwortloses Microsoft-Konto

Windows nimmt bei Remotedesktop **kein Windows Hello**: keine PIN, kein Gesicht, kein Fingerabdruck. Es
braucht ein Kennwort. Meldest du dich am PC nur mit PIN an, hast du zwei Wege:

**Microsoft-Konto mit Kennwort (dein gewohnter Desktop).** Hat dein Konto noch ein Kennwort, gib in Sesam
die E-Mail-Adresse und dieses Kennwort ein. Hast du auf account.microsoft.com die **kennwortlose
Anmeldung** eingeschaltet, gibt es kein Kennwort mehr; dann dort unter **Sicherheit › Erweiterte
Sicherheitsoptionen** die kennwortlose Anmeldung wieder ausschalten und ein Kennwort setzen. Danach
einmal am PC mit diesem Kennwort anmelden (Anmeldeoptionen › Schlüssel), damit Windows es kennt.

**Eigenes lokales Konto nur für den Fernzugriff (Microsoft-Konto bleibt kennwortlos).** In einer
PowerShell als Administrator:

```powershell
$pw = Read-Host -AsSecureString "Kennwort für den Fernzugriff"
New-LocalUser -Name "fernzugriff" -Password $pw -PasswordNeverExpires
Add-LocalGroupMember -SID "S-1-5-32-555" -Member "fernzugriff"
```

In Sesam dann Benutzer `fernzugriff` und dieses Kennwort. Das ist ein eigener Desktop, nicht dein
angemeldeter; dafür bleibt dein Microsoft-Konto ohne Kennwort. `S-1-5-32-555` ist die Gruppe
Remotedesktopbenutzer in jeder Windows-Sprache.
