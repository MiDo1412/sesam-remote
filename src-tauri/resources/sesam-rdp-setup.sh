#!/bin/bash
# Sesam Remote: RDP-Fernzugriff auf Ubuntu mit GNOME (ab GNOME 46 / Ubuntu 24.04) einrichten.
# Aufruf:  sudo bash sesam-rdp-setup.sh
# Kann beliebig oft laufen. Jeder Schritt zeigt den aktuellen Stand; Enter lässt alles, wie es ist.
set -euo pipefail
[ "$(id -u)" = 0 ] || { echo "Bitte mit sudo starten: sudo bash $0"; exit 1; }
command -v grdctl >/dev/null || { echo "GNOME Remote Desktop fehlt: sudo apt install gnome-remote-desktop"; exit 1; }

CONF=/etc/sesam-rdp/ports          # Zeilen: <benutzer> <port>
USER_SCRIPT=/usr/local/bin/sesam-rdp-user
UNIT=/etc/systemd/user/sesam-rdp-user.service
mkdir -p /etc/sesam-rdp; touch "$CONF"

# --- Darstellung ---------------------------------------------------------------------------------
if [ -t 1 ]; then B=$'\e[1m' D=$'\e[2m' G=$'\e[32m' Y=$'\e[33m' C=$'\e[36m' N=$'\e[0m'; else B= D= G= Y= C= N=; fi
STEPS=5
titel()  { echo; echo "${B}${C}── Schritt $1/$STEPS · $2 ${N}${C}$(printf '─%.0s' $(seq 1 $((60 - ${#2}))))${N}"; echo "${D}$3${N}"; }
stand()  { printf '  %-22s %s\n' "$1" "$2"; }
ok()     { echo "  ${G}✓${N} $*"; }
hinweis(){ echo "  ${Y}!${N} $*"; }
gut()    { echo "${G}$*${N}"; }
warn()   { echo "${Y}$*${N}"; }
# frage "Frage" "was j bewirkt" "was Enter bewirkt" -> 0 bei j
frage()  { local a; read -rp "  ${B}$1${N}  ${D}[j] $2 · [Enter] $3:${N} " a; [[ $a =~ ^[jJyY] ]]; }

# --- Helfer --------------------------------------------------------------------------------------
human_users() { getent passwd | awk -F: '$3>=1000 && $3<60000 && $7 !~ /(nologin|false)$/ {print $1}'; }
port_of() { awk -v u="$1" '$1==u {print $2}' "$CONF"; }
autologin_user() { awk -F= '/^AutomaticLoginEnable *= *[Tt]rue/{e=1} /^AutomaticLogin *=/{u=$2} END{if(e) print u}' /etc/gdm3/custom.conf 2>/dev/null | tr -d ' '; }
home_of() { getent passwd "$1" | cut -d: -f6; }
as_user() { # $1 Benutzer, Rest: Befehl in dessen laufender Sitzung
  local u=$1 uid; uid=$(id -u "$u"); shift
  runuser -u "$u" -- env DBUS_SESSION_BUS_ADDRESS="unix:path=/run/user/$uid/bus" XDG_RUNTIME_DIR="/run/user/$uid" HOME="$(home_of "$u")" "$@"
}
logged_in() { # hat der Benutzer eine grafische Sitzung (nicht nur SSH)?
  local s
  for s in $(loginctl list-sessions --no-legend | awk -v u="$1" '$3==u {print $1}'); do
    case $(loginctl show-session "$s" -p Type --value) in wayland|x11) return 0;; esac
  done
  return 1
}
fingerprint() { openssl x509 -in "$1" -noout -fingerprint -sha256 2>/dev/null | cut -d= -f2; }
# grdctl-Status lesen: "an"/"aus" und "gesetzt"/"fehlen"
grd_on()    { grep -qE '^\s*Status: enabled' <<<"$1" && echo an || echo aus; }
grd_creds() { grep -qE '^\s*Username: \(hidden\)|^\s*Username: [^(]' <<<"$1" && echo gesetzt || echo fehlen; }
sys_status() { grdctl --system status 2>/dev/null || true; }
user_status() { as_user "$1" grdctl status 2>/dev/null || true; }
vm_state() {
  local f=/etc/default/grub.d/99-sesam-virtual-monitor.cfg old=/etc/default/grub.d/99-virtual-monitor.cfg out
  out=$(grep -o 'drm.edid_firmware=[^:]*' /proc/cmdline | cut -d= -f2 || true)
  if [ -n "$out" ]; then echo "aktiv ($out)"
  elif [ -e "$f" ] || [ -e "$old" ]; then echo "eingerichtet, wirkt nach dem nächsten Neustart"
  else echo "nicht eingerichtet"; fi
}
SYS_CERT="$(home_of gnome-remote-desktop)/.local/share/gnome-remote-desktop/certificates/rdp-tls.crt"
user_cert() { echo "$(home_of "$1")/.local/share/gnome-remote-desktop/certificates/rdp-tls.crt"; }
AUTO=$(autologin_user)

# --- Kopf ----------------------------------------------------------------------------------------
clear 2>/dev/null || true
echo "${B}Sesam Remote · Fernzugriff einrichten${N}   $(hostname) · $(hostname -I | awk '{print $1}')"
echo
echo "Es gibt zwei Arten, diesen PC aus Sesam zu erreichen:"
echo "  ${B}Anmeldebildschirm${N}    Port 3389          Als beliebiger Benutzer anmelden, eigener Desktop im Hintergrund."
echo "  ${B}Sitzung übernehmen${N}   ein Port je Benutzer  Sehen und steuern, was gerade am Bildschirm läuft."
echo
echo "Bedienung: Jeder Schritt zeigt zuerst den aktuellen Stand."
echo "  ${B}j${N} = einschalten oder ändern   ${B}Enter${N} = so lassen. ${B}Es wird nie etwas ausgeschaltet.${N}"

# --- 1. Grundlage: Sitzung übernehmen für alle Benutzer ------------------------------------------
titel 1 "Grundlage" "Bereitet \"Sitzung übernehmen\" für alle Benutzer vor (eigenes Zertifikat, fester Port). Läuft automatisch, keine Frage."
cat > "$USER_SCRIPT" <<'EOF'
#!/bin/sh
# Sesam Remote: RDP-Bildschirmfreigabe für die eigene Sitzung einschalten (eigenes Zertifikat, fester Port aus /etc/sesam-rdp/ports).
# Die Zugangsdaten setzt jeder Benutzer selbst: Einstellungen > System > Remotedesktop > Desktopfreigabe.
set -e
# nur echte Benutzer, nicht gdm-greeter & Co.
uid=$(id -u); { [ "$uid" -ge 1000 ] && [ "$uid" -lt 60000 ]; } || exit 0
D="$HOME/.local/share/gnome-remote-desktop/certificates"
if [ ! -s "$D/rdp-tls.crt" ]; then
  mkdir -p "$D"
  openssl req -new -newkey rsa:4096 -days 3650 -nodes -x509 -subj "/CN=$(hostname)-$(id -un)" \
    -keyout "$D/rdp-tls.key" -out "$D/rdp-tls.crt" 2>/dev/null
  chmod 600 "$D/rdp-tls.key"
fi
grdctl rdp set-tls-cert "$D/rdp-tls.crt"
grdctl rdp set-tls-key "$D/rdp-tls.key"
grdctl rdp disable-view-only
PORT=$(awk -v u="$(id -un)" '$1==u {print $2}' /etc/sesam-rdp/ports 2>/dev/null)
if [ -n "$PORT" ]; then grdctl rdp set-port "$PORT"; grdctl rdp disable-port-negotiation
else grdctl rdp enable-port-negotiation; fi
grdctl rdp enable
EOF
chmod 755 "$USER_SCRIPT"
cat > "$UNIT" <<EOF
[Unit]
Description=Sesam Remote: RDP-Bildschirmfreigabe für diese Sitzung
Before=gnome-remote-desktop.service
PartOf=graphical-session.target
After=graphical-session-pre.target

[Service]
Type=oneshot
ExecStart=$USER_SCRIPT

[Install]
WantedBy=graphical-session.target
EOF
if [ -e /etc/systemd/user/grd-user-setup.service ]; then   # frühere Version ablösen
  systemctl --global disable grd-user-setup.service >/dev/null 2>&1 || true
  rm -f /etc/systemd/user/grd-user-setup.service /usr/local/bin/grd-user-setup
fi
systemctl --global enable sesam-rdp-user.service gnome-remote-desktop.service >/dev/null 2>&1
ok "Erledigt. Greift bei jeder Anmeldung eines Benutzers."

# --- 2. Anmeldebildschirm (Port 3389) ------------------------------------------------------------
titel 2 "Anmeldebildschirm (Port 3389)" "In Sesam zuerst diese RDP-Zugangsdaten, danach der normale Ubuntu-Anmeldebildschirm. Tipp: Benutzer und Passwort eines Ubuntu-Benutzers nehmen, dann ist es wie bei Windows."
S=$(sys_status); ON=$(grd_on "$S"); CR=$(grd_creds "$S")
stand "Stand:" "$( [ "$ON" = an ] && gut eingeschaltet || warn ausgeschaltet ) · Zugangsdaten $( [ "$CR" = gesetzt ] && gut gesetzt || warn fehlen )"
set_sys_creds() {
  # ohne Argumente: grdctl fragt selbst; als Argument landete das Passwort über pkexec im Systemprotokoll
  echo "  Benutzername und Passwort für Port 3389 eingeben:"
  grdctl --system rdp set-credentials && ok "Zugangsdaten gespeichert."
}
if [ "$ON" = aus ]; then
  if frage "Anmeldebildschirm einschalten?" "einschalten" "aus lassen"; then
    GRDDIR=$(dirname "$SYS_CERT")
    if [ ! -s "$SYS_CERT" ]; then
      runuser -u gnome-remote-desktop -- sh -c "mkdir -p '$GRDDIR' && openssl req -new -newkey rsa:4096 -days 3650 -nodes -x509 \
        -subj '/CN=$(hostname)' -keyout '$GRDDIR/rdp-tls.key' -out '$SYS_CERT' 2>/dev/null && chmod 600 '$GRDDIR/rdp-tls.key'"
    fi
    grdctl --system rdp set-tls-cert "$SYS_CERT" 2>/dev/null
    grdctl --system rdp set-tls-key "$GRDDIR/rdp-tls.key" 2>/dev/null
    [ "$CR" = gesetzt ] || set_sys_creds
    grdctl --system rdp enable 2>/dev/null
    systemctl enable --now gnome-remote-desktop.service >/dev/null 2>&1
    ok "Anmeldebildschirm ist eingeschaltet."
  fi
elif [ "$CR" = fehlen ]; then
  frage "Zugangsdaten jetzt setzen? Ohne geht keine Verbindung." "setzen" "später" && set_sys_creds
else
  frage "Zugangsdaten ändern?" "neue eingeben" "behalten" && set_sys_creds
fi

# --- 3. Feste Ports für "Sitzung übernehmen" -----------------------------------------------------
titel 3 "Ports für \"Sitzung übernehmen\"" "Jeder Benutzer bekommt einen festen Port. Port 3389 ist für den Anmeldebildschirm reserviert."
if [ ! -s "$CONF" ]; then   # erster Lauf: Vorschlag, automatisch angemeldeter Benutzer zuerst
  next=3390
  for u in $AUTO $(human_users | grep -vx "${AUTO:-#}"); do echo "$u $next" >> "$CONF"; next=$((next+1)); done
  ok "Ports vorgeschlagen und gespeichert, unten änderbar."
fi
show_ports() { for u in $(human_users); do p=$(port_of "$u"); stand "$u" "${p:-${Y}kein fester Port${N}}"; done; }
show_ports
while :; do
  read -rp "  ${B}Port setzen oder ändern?${N}  ${D}Benutzername eingeben · [Enter] weiter:${N} " u || break
  [ -n "$u" ] || break
  id "$u" >/dev/null 2>&1 || { hinweis "Benutzer \"$u\" gibt es nicht."; continue; }
  read -rp "  Port für $u ${D}(1024–65535, [Enter] = keinen festen Port):${N} " p || p=
  if [ -z "$p" ]; then sed -i "/^$u /d" "$CONF"; ok "$u: kein fester Port (GNOME wählt einen freien)."; continue; fi
  if ! [[ $p =~ ^[0-9]+$ ]] || (( p < 1024 || p > 65535 || p == 3389 )); then hinweis "Ungültiger Port."; continue; fi
  if awk -v p="$p" -v u="$u" '$2==p && $1!=u {f=1} END{exit !f}' "$CONF"; then hinweis "Port $p hat schon ein anderer Benutzer."; continue; fi
  sed -i "/^$u /d" "$CONF"; echo "$u $p" >> "$CONF"; ok "$u: Port $p"
done

# --- 4. Zugangsdaten für "Sitzung übernehmen" ----------------------------------------------------
titel 4 "Zugangsdaten für \"Sitzung übernehmen\"" "Jeder Benutzer hat eigene. GNOME speichert sie in seinem Schlüsselbund, darum geht das nur, solange er am Bildschirm angemeldet ist."
LATER=()
keyring_encrypted() { # 0 = Schlüsselbund hat ein Passwort (ein unverschlüsselter ist Klartext und beginnt mit "[keyring]")
  local kr; kr="$(home_of "$1")/.local/share/keyrings/login.keyring"
  [ -f "$kr" ] && [ "$(head -c 12 "$kr")" = "GnomeKeyring" ]
}
keyring_remove_password() { # $1 Benutzer; geht gesperrt und entsperrt
  local KP
  read -rsp "    Bisheriges Schlüsselbund-Passwort (meist das Ubuntu-Passwort von $1, vor einer allfälligen Änderung): " KP; echo
  # Passwort über stdin, nie als Argument (sonst landet es im Protokoll)
  printf '%s\n' "$KP" | as_user "$1" python3 -c '
import sys
from gi.repository import Gio, GLib
bus = Gio.bus_get_sync(Gio.BusType.SESSION, None)
def call(iface, method, args, ret=None):
    return bus.call_sync("org.freedesktop.secrets", "/org/freedesktop/secrets", iface, method,
                         args, GLib.VariantType(ret) if ret else None, 0, -1, None)
sess = call("org.freedesktop.Secret.Service", "OpenSession",
            GLib.Variant("(sv)", ("plain", GLib.Variant("s", ""))), "(vo)").unpack()[1]
sec = lambda p: (sess, b"", p.encode(), "text/plain")
col, I = "/org/freedesktop/secrets/collection/login", "org.gnome.keyring.InternalUnsupportedGuiltRiddenInterface"
try:
    call(I, "ChangeWithMasterPassword", GLib.Variant("(o(oayays)(oayays))", (col, sec(sys.stdin.readline().rstrip("\n")), sec(""))))
    call(I, "UnlockWithMasterPassword", GLib.Variant("(o(oayays))", (col, sec(""))))
except GLib.Error as e:
    sys.exit("    Schlüsselbund nicht geändert (Passwort falsch?): " + e.message)
' && ok "Schlüsselbund von $1 ist jetzt ohne Passwort."
}
for u in $(human_users); do
  if ! logged_in "$u"; then LATER+=("$u"); continue; fi
  restart_user() { as_user "$u" systemctl --user restart sesam-rdp-user.service gnome-remote-desktop.service >/dev/null 2>&1 || true; }
  CR=$(grd_creds "$(user_status "$u")")
  stand "$u" "angemeldet · Zugangsdaten $( [ "$CR" = gesetzt ] && gut gesetzt || warn "fehlen oder Schlüsselbund gesperrt" )"
  if [ "$u" = "$AUTO" ]; then
    # Bei automatischer Anmeldung zählt, ob der Schlüsselbund ein Passwort hat, nicht ob er gerade offen ist:
    # nach jedem Neustart bleibt ein Schlüsselbund mit Passwort gesperrt.
    if keyring_encrypted "$u"; then
      stand "" "Schlüsselbund: $(warn "mit Passwort, nach Neustart nicht lesbar")"
      hinweis "Schlüsselbund von $u hat ein Passwort. Nach jedem Neustart kann GNOME die Zugangsdaten nicht lesen."
      echo "    Abhilfe: Schlüsselbund ohne Passwort. Gespeicherte Passwörter von $u liegen dann unverschlüsselt auf der Platte."
      if frage "Schlüsselbund von $u ohne Passwort speichern?" "ja, nötig für den Fernzugriff" "so lassen"; then
        keyring_remove_password "$u" && { restart_user; CR=$(grd_creds "$(user_status "$u")"); } || true
      fi
    else
      stand "" "Schlüsselbund: $(gut "ohne Passwort (gut für automatische Anmeldung)")"
    fi
  fi
  if [ "$CR" = gesetzt ]; then q="Zugangsdaten für $u ändern?"; a="neue eingeben"; e="behalten"
  else q="Zugangsdaten für $u setzen?"; a="setzen"; e="später"; fi
  if ! frage "$q" "$a" "$e"; then restart_user; continue; fi
  if [ "$u" != "$AUTO" ]; then
    locked=$(as_user "$u" busctl --user get-property org.freedesktop.secrets /org/freedesktop/secrets/collection/login \
             org.freedesktop.Secret.Collection Locked 2>/dev/null || true)
    if [ "$locked" = "b true" ]; then hinweis "Der Schlüsselbund von $u ist gesperrt. $u bitte einmal von Hand anmelden, dann erneut starten."; continue; fi
  elif keyring_encrypted "$u"; then
    hinweis "Ohne Passwort-freien Schlüsselbund gelten die Zugangsdaten nur bis zum nächsten Neustart."
  fi
  echo "  Benutzername und Passwort für \"Sitzung von $u übernehmen\" eingeben (Tipp: Ubuntu-Benutzer und -Passwort von $u):"
  as_user "$u" grdctl rdp set-credentials
  restart_user
  ok "Zugangsdaten für $u gespeichert."
done
if [ ${#LATER[@]} -gt 0 ]; then
  stand "${LATER[*]}" "nicht angemeldet, darum hier übersprungen"
  echo "  ${D}Später: Benutzer anmelden (am Bildschirm oder über Port 3389), dann dieses Skript erneut starten"
  echo "  oder in seinen Einstellungen unter System > Remotedesktop > Desktopfreigabe setzen.${N}"
fi

# --- 5. Virtueller Monitor -----------------------------------------------------------------------
titel 5 "Virtueller Monitor" "Damit der PC auch mit ausgeschaltetem oder abgestecktem Bildschirm ein Bild liefert. Nur nötig, wenn er ohne Bildschirm laufen soll."
VM=$(vm_state)
stand "Stand:" "$( [ "$VM" = "nicht eingerichtet" ] && warn "$VM" || gut "$VM" )"
if [ "$VM" = "nicht eingerichtet" ]; then q="Virtuellen Monitor einrichten?"; a="einrichten"; e="nicht einrichten"
else q="Neu einrichten (z. B. nach Monitorwechsel)?"; a="neu einrichten"; e="so lassen"; fi
if frage "$q" "$a" "$e"; then
  mapfile -t CON < <(for c in /sys/class/drm/card*-*; do [ "$(cat "$c/status" 2>/dev/null)" = connected ] && basename "$c"; done)
  if [ ${#CON[@]} -eq 0 ]; then
    hinweis "Kein Bildschirm erkannt. Bitte einmal anschliessen und das Skript erneut starten."
  else
    c=${CON[0]}
    if [ ${#CON[@]} -gt 1 ]; then read -rp "  Angeschlossen: ${CON[*]}. Welcher Ausgang? [${CON[0]}] " a; c=${a:-$c}; fi
    name=${c#card*-}
    edid=/usr/lib/firmware/edid/sesam-$name.bin
    install -D -m644 "/sys/class/drm/$c/edid" "$edid"
    rm -f /etc/default/grub.d/99-virtual-monitor.cfg /etc/dracut.conf.d/99-virtual-monitor.conf /etc/initramfs-tools/hooks/edid
    if command -v dracut >/dev/null; then
      echo "install_items+=\" $edid \"" > /etc/dracut.conf.d/99-sesam-virtual-monitor.conf
    else
      printf '#!/bin/sh\n[ "$1" = prereqs ] && exit 0\n. /usr/share/initramfs-tools/hook-functions\ncopy_file firmware %s\n' "$edid" \
        > /etc/initramfs-tools/hooks/sesam-edid
      chmod 755 /etc/initramfs-tools/hooks/sesam-edid
    fi
    echo "GRUB_CMDLINE_LINUX_DEFAULT=\"\$GRUB_CMDLINE_LINUX_DEFAULT drm.edid_firmware=$name:edid/sesam-$name.bin video=$name:e\"" \
      > /etc/default/grub.d/99-sesam-virtual-monitor.cfg
    echo "  Bitte warten, Startdateien werden neu erstellt …"
    update-initramfs -u >/dev/null 2>&1
    update-grub >/dev/null 2>&1
    ok "Virtueller Monitor auf $name eingerichtet. Wirkt nach dem nächsten Neustart."
  fi
fi

# --- Firewall (nur wenn aktiv) -------------------------------------------------------------------
if command -v ufw >/dev/null && ufw status | head -1 | grep -qE ': (aktiv|active)$'; then
  echo
  hinweis "Die Firewall ist aktiv."
  if frage "Port 3389 und die festen Ports freigeben?" "freigeben" "so lassen"; then
    ufw allow 3389/tcp >/dev/null
    while read -r _ p; do [ -n "$p" ] && ufw allow "$p/tcp" >/dev/null; done < "$CONF"
    ok "Ports freigegeben."
  fi
fi

# --- Übersicht -----------------------------------------------------------------------------------
echo
echo "${B}${C}══ Übersicht für Sesam · $(hostname) · $(hostname -I | awk '{print $1}') ══${N}"
S=$(sys_status)
if [ "$(grd_on "$S")" = an ] && [ "$(grd_creds "$S")" = gesetzt ]; then st=$(gut bereit)
elif [ "$(grd_on "$S")" = an ]; then st=$(warn "Zugangsdaten fehlen"); else st=$(warn ausgeschaltet); fi
printf '  %s%-6s%s %-36s %s\n' "$B" 3389 "$N" "Anmeldebildschirm" "$st"
echo "  ${D}       Fingerabdruck $(fingerprint "$SYS_CERT")${N}"
while read -r u p; do
  [ -n "$u" ] || continue
  if ! logged_in "$u"; then st=$(warn "nur wenn $u angemeldet ist")
  elif [ "$(grd_creds "$(user_status "$u")")" = gesetzt ]; then st=$(gut bereit)
  else st=$(warn "Zugangsdaten fehlen"); fi
  printf '  %s%-6s%s %-36s %s\n' "$B" "$p" "$N" "Sitzung von $u übernehmen" "$st"
  fp=$(fingerprint "$(user_cert "$u")")
  echo "  ${D}       Fingerabdruck ${fp:-entsteht bei der ersten Anmeldung von $u}${N}"
done < "$CONF"
echo
echo "  ${D}Den Fingerabdruck zeigt Sesam beim ersten Verbinden. Stimmt er überein, bestätigen.${N}"
echo "  ${D}Virtueller Monitor: $(vm_state)${N}"
