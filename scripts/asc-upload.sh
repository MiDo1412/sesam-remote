#!/bin/sh
# Lädt eine IPA zu App Store Connect hoch (landet in TestFlight, wird nicht zur Prüfung eingereicht).
# Keine Geheimnisse im Repository: Key-ID und Issuer-ID stehen in ~/.sesam/asc.env,
# der private Schlüssel (.p8) im Schlüsselbund (Dienst «Sesam AppStore Connect API», Konto = Key-ID).
#
#   scripts/asc-upload.sh ["src-tauri/gen/apple/build/arm64/Sesam Remote.ipa"]
#   scripts/asc-upload.sh path/to/sesam-remote.xcarchive   # signiert und exportiert zuerst (Cloud-Signatur mit dem
#                                                          # Schlüssel, Rolle Admin), ohne Apple-ID in Xcode
set -eu

IPA=${1:-"src-tauri/gen/apple/build/arm64/Sesam Remote.ipa"}
ENV="$HOME/.sesam/asc.env"
SVC="Sesam AppStore Connect API"

[ -e "$IPA" ] || { echo "Fehlt: $IPA" >&2; exit 1; }
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

# Archiv: «tauri ios build --export-method app-store-connect» scheitert ohne Apple-ID in Xcode, xcodebuild kann mit dem Schlüssel
case "$IPA" in *.xcarchive)
  printf '%s\n' '<?xml version="1.0" encoding="UTF-8"?><!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd"><plist version="1.0"><dict><key>method</key><string>app-store-connect</string><key>signingStyle</key><string>automatic</string></dict></plist>' > "$DIR/export.plist"
  xcodebuild -exportArchive -archivePath "$IPA" -exportPath "$DIR/out" -exportOptionsPlist "$DIR/export.plist" -allowProvisioningUpdates \
    -authenticationKeyPath "$DIR/AuthKey_$ASC_KEY_ID.p8" -authenticationKeyID "$ASC_KEY_ID" -authenticationKeyIssuerID "$ASC_ISSUER_ID"
  IPA=$(ls "$DIR"/out/*.ipa | head -n 1) ;;
esac

API_PRIVATE_KEYS_DIR="$DIR" xcrun altool --upload-app -f "$IPA" -t ios \
  --apiKey "$ASC_KEY_ID" --apiIssuer "$ASC_ISSUER_ID"
