#!/bin/sh
# Einmalig: Signaturschlüssel für die automatischen Updates erzeugen (docs/release.md).
# Privater Schlüssel: ~/.sesam/updater.key (mit Passwort), Passwort: Schlüsselbund («Sesam Remote Updater»),
# öffentlicher Schlüssel: src-tauri/tauri.conf.json (plugins.updater.pubkey). Nichts davon wird ausgegeben.
set -eu
cd "$(dirname "$0")/.."
KEY="$HOME/.sesam/updater.key"
if [ -e "$KEY" ]; then echo "Schlüssel gibt es schon: $KEY (nicht überschrieben)"; exit 1; fi
mkdir -p "$HOME/.sesam" && chmod 700 "$HOME/.sesam"
PW=$(openssl rand -base64 32 | tr -d '\n')
security add-generic-password -s "Sesam Remote Updater" -a updater -w "$PW" -T /usr/bin/security
npx tauri signer generate -w "$KEY" -p "$PW" --ci >/dev/null
unset PW
chmod 600 "$KEY"
node -e '
const fs=require("fs"),p="src-tauri/tauri.conf.json",c=JSON.parse(fs.readFileSync(p,"utf8"));
c.plugins.updater.pubkey=fs.readFileSync(process.env.HOME+"/.sesam/updater.key.pub","utf8").trim();
fs.writeFileSync(p,JSON.stringify(c,null,2)+"\n");'
echo "Fertig. Öffentlicher Schlüssel steht in src-tauri/tauri.conf.json; bitte committen."
echo "Sichern: ~/.sesam/updater.key und das Passwort aus dem Schlüsselbund («Sesam Remote Updater») an einen sicheren Ort (docs/release.md)."
