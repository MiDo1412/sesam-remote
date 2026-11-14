#!/bin/sh
# Lädt eine IPA zu App Store Connect hoch (landet in TestFlight, wird nicht zur Prüfung eingereicht).
# Keine Geheimnisse im Repository: Key-ID und Issuer-ID stehen in ~/.sesam/asc.env,
# der private Schlüssel (.p8) im Schlüsselbund (Dienst «Sesam AppStore Connect API», Konto = Key-ID).
#
#   scripts/asc-upload.sh ["src-tauri/gen/apple/build/arm64/Sesam Remote.ipa"]
set -eu

IPA=${1:-"src-tauri/gen/apple/build/arm64/Sesam Remote.ipa"}
ENV="$HOME/.sesam/asc.env"
SVC="Sesam AppStore Connect API"

[ -f "$IPA" ] || { echo "IPA fehlt: $IPA" >&2; exit 1; }
[ -f "$ENV" ] || { echo "Fehlt: $ENV (ASC_KEY_ID, ASC_ISSUER_ID)" >&2; exit 1; }
. "$ENV"
: "${ASC_KEY_ID:?ASC_KEY_ID fehlt in $ENV}" "${ASC_ISSUER_ID:?ASC_ISSUER_ID fehlt in $ENV}"

# Schlüssel nur für die Dauer des Uploads als Datei, nur für diesen Benutzer lesbar
umask 077
DIR=$(mktemp -d)
trap 'rm -rf "$DIR"' EXIT INT TERM
KEY=$(security find-generic-password -a "$ASC_KEY_ID" -s "$SVC" -w) || { echo "Schlüssel nicht im Schlüsselbund" >&2; exit 1; }
case "$KEY" in
  -----BEGIN*) printf '%s\n' "$KEY" > "$DIR/AuthKey_$ASC_KEY_ID.p8" ;;
  *) printf '%s' "$KEY" | xxd -r -p > "$DIR/AuthKey_$ASC_KEY_ID.p8" ;;  # security gibt Daten mit Zeilenumbrüchen als Hex aus
esac
grep -q 'BEGIN PRIVATE KEY' "$DIR/AuthKey_$ASC_KEY_ID.p8" || { echo "Schlüssel im Schlüsselbund ist kein .p8" >&2; exit 1; }

API_PRIVATE_KEYS_DIR="$DIR" xcrun altool --upload-app -f "$IPA" -t ios \
  --apiKey "$ASC_KEY_ID" --apiIssuer "$ASC_ISSUER_ID"
