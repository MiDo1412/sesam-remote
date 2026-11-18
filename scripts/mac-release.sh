#!/bin/sh
# Baut die Mac-Version für den Release: signiert mit «Developer ID Application», notarisiert bei Apple
# (Tauri heftet das Ticket an), dazu Updater-Signatur, portables ZIP und latest.json.
# Keine Geheimnisse im Repository: Updater-Schlüssel wie in docs/release.md, App-Store-Connect-Schlüssel
# wie bei scripts/asc-upload.sh (~/.sesam/asc.env + Schlüsselbund «Sesam AppStore Connect API»).
#
#   scripts/mac-release.sh
set -eu

ENV="$HOME/.sesam/asc.env"
SVC="Sesam AppStore Connect API"
[ -f "$ENV" ] || { echo "Fehlt: $ENV (ASC_KEY_ID, ASC_ISSUER_ID)" >&2; exit 1; }
. "$ENV"
: "${ASC_KEY_ID:?ASC_KEY_ID fehlt in $ENV}" "${ASC_ISSUER_ID:?ASC_ISSUER_ID fehlt in $ENV}"

# Ohne Developer ID wäre die App nur ad hoc signiert: macOS fragt dann nach jedem Update erneut nach dem Schlüsselbund.
ID=$(security find-identity -v -p codesigning | sed -n 's/.*"\(Developer ID Application: [^"]*\)".*/\1/p' | head -n 1)
[ -n "$ID" ] || { echo "Kein Zertifikat «Developer ID Application» im Schlüsselbund" >&2; exit 1; }

umask 077
DIR=$(mktemp -d)
trap 'rm -rf "$DIR"' EXIT INT TERM
KEY=$(security find-generic-password -a "$ASC_KEY_ID" -s "$SVC" -w) || { echo "Schlüssel nicht im Schlüsselbund" >&2; exit 1; }
case "$KEY" in
  -----BEGIN*) printf '%s\n' "$KEY" > "$DIR/AuthKey_$ASC_KEY_ID.p8" ;;
  *) printf '%s' "$KEY" | xxd -r -p > "$DIR/AuthKey_$ASC_KEY_ID.p8" ;;  # security gibt Daten mit Zeilenumbrüchen als Hex aus
esac
grep -q 'BEGIN PRIVATE KEY' "$DIR/AuthKey_$ASC_KEY_ID.p8" || { echo "Schlüssel im Schlüsselbund ist kein .p8" >&2; exit 1; }

export APPLE_API_KEY="$ASC_KEY_ID" APPLE_API_ISSUER="$ASC_ISSUER_ID" APPLE_API_KEY_PATH="$DIR/AuthKey_$ASC_KEY_ID.p8"
export TAURI_SIGNING_PRIVATE_KEY="$HOME/.sesam/updater.key"
TAURI_SIGNING_PRIVATE_KEY_PASSWORD="$(security find-generic-password -s 'Sesam Remote Updater' -a updater -w)"
export TAURI_SIGNING_PRIVATE_KEY_PASSWORD

npx tauri build --bundles app,dmg \
  --config "{\"bundle\":{\"createUpdaterArtifacts\":true,\"macOS\":{\"signingIdentity\":\"$ID\"}}}"
unset TAURI_SIGNING_PRIVATE_KEY_PASSWORD

APP="src-tauri/target/release/bundle/macos/Sesam Remote.app"
codesign --verify --deep --strict "$APP"
xcrun stapler validate "$APP"
spctl -a -t exec -vv "$APP"

node scripts/portable.mjs
node scripts/updates.mjs
