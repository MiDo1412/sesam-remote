#!/bin/bash
# Sesam Remote: RDP-Fernzugriff auf Ubuntu mit GNOME (ab GNOME 46) einrichten.
# Aufruf:  sudo bash sesam-rdp-setup.sh
# Kann beliebig oft laufen. Jeder Schritt fragt vorher nach.
#
#   Port 3389       Anmeldebildschirm: als beliebiger Benutzer anmelden, eigener Desktop im Hintergrund
#   feste Ports     Bildschirmfreigabe: die Sitzung übernehmen, die am Bildschirm läuft (pro Benutzer ein Port)
set -euo pipefail
[ "$(id -u)" = 0 ] || { echo "Bitte mit sudo starten: sudo bash $0"; exit 1; }
command -v grdctl >/dev/null || { echo "GNOME Remote Desktop fehlt: sudo apt install gnome-remote-desktop"; exit 1; }

CONF=/etc/sesam-rdp/ports          # Zeilen: <benutzer> <port>
USER_SCRIPT=/usr/local/bin/sesam-rdp-user
UNIT=/etc/systemd/user/sesam-rdp-user.service
mkdir -p /etc/sesam-rdp; touch "$CONF"

ja() { local a; read -rp "$1 [j/N] " a; [[ $a =~ ^[jJyY] ]]; }
human_users() { getent passwd | awk -F: '$3>=1000 && $3<60000 && $7 !~ /(nologin|false)$/ {print $1}'; }
port_of() { awk -v u="$1" '$1==u {print $2}' "$CONF"; }
autologin_user() { awk -F= '/^AutomaticLoginEnable *= *[Tt]rue/{e=1} /^AutomaticLogin *=/{u=$2} END{if(e) print u}' /etc/gdm3/custom.conf 2>/dev/null | tr -d ' '; }
as_user() { # $1 Benutzer, Rest: Befehl in dessen laufender Sitzung
  local u=$1 uid; uid=$(id -u "$u"); shift
  runuser -u "$u" -- env DBUS_SESSION_BUS_ADDRESS="unix:path=/run/user/$uid/bus" XDG_RUNTIME_DIR="/run/user/$uid" HOME="$(getent passwd "$u" | cut -d: -f6)" "$@"
}
logged_in() { # hat der Benutzer eine grafische Sitzung (nicht nur SSH)?
  local s
  for s in $(loginctl list-sessions --no-legend | awk -v u="$1" '$3==u {print $1}'); do
    case $(loginctl show-session "$s" -p Type --value) in wayland|x11) return 0;; esac
  done
  return 1
}
fingerprint() { openssl x509 -in "$1" -noout -fingerprint -sha256 2>/dev/null | cut -d= -f2; }

# --- 1. Bildschirmfreigabe für alle Benutzer (läuft bei jeder Anmeldung) -------------------------
cat > "$USER_SCRIPT" <<'EOF'
#!/bin/sh
# Sesam Remote: RDP-Bildschirmfreigabe für die eigene Sitzung einschalten (eigenes Zertifikat, fester Port aus /etc/sesam-rdp/ports).
# Die Zugangsdaten setzt jeder Benutzer selbst: Einstellungen > System > Remotedesktop > Desktopfreigabe.
set -e
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
# frühere Version dieses Skripts ablösen
if [ -e /etc/systemd/user/grd-user-setup.service ]; then
  systemctl --global disable grd-user-setup.service >/dev/null 2>&1 || true
  rm -f /etc/systemd/user/grd-user-setup.service /usr/local/bin/grd-user-setup
fi
systemctl --global enable sesam-rdp-user.service gnome-remote-desktop.service >/dev/null 2>&1
echo "✓ Bildschirmfreigabe ist für alle Benutzer eingeschaltet (greift bei der Anmeldung)."

# --- 2. Anmeldebildschirm (Port 3389) ------------------------------------------------------------
echo
if ja "Anmeldebildschirm per RDP einschalten (Port 3389, für alle Benutzer)?"; then
  GRDHOME=$(getent passwd gnome-remote-desktop | cut -d: -f6)
  C="$GRDHOME/.local/share/gnome-remote-desktop/certificates"
  if [ ! -s "$C/rdp-tls.crt" ]; then
    runuser -u gnome-remote-desktop -- sh -c "mkdir -p '$C' && openssl req -new -newkey rsa:4096 -days 3650 -nodes -x509 \
      -subj '/CN=$(hostname)' -keyout '$C/rdp-tls.key' -out '$C/rdp-tls.crt' 2>/dev/null && chmod 600 '$C/rdp-tls.key'"
  fi
  grdctl --system rdp set-tls-cert "$C/rdp-tls.crt" 2>/dev/null
  grdctl --system rdp set-tls-key "$C/rdp-tls.key" 2>/dev/null
  echo "Diese Zugangsdaten gibst du in Sesam ein, danach folgt der normale Ubuntu-Anmeldebildschirm."
  echo "Tipp: Benutzername und Passwort eines Ubuntu-Benutzers verwenden, dann ist es wie bei Windows."
  if ja "Zugangsdaten für Port 3389 setzen oder ändern?"; then
    # ohne Argumente: grdctl fragt selbst; als Argument landete das Passwort über pkexec im Systemprotokoll
    echo "Benutzername und Passwort für den Anmeldebildschirm (3389) eingeben:"
    grdctl --system rdp set-credentials
  fi
  grdctl --system rdp enable 2>/dev/null
  systemctl enable --now gnome-remote-desktop.service >/dev/null 2>&1
  echo "✓ Anmeldebildschirm auf Port 3389 ist eingeschaltet."
fi

# --- 3. Feste Ports für die Bildschirmfreigabe ---------------------------------------------------
echo
AUTO=$(autologin_user)
if [ ! -s "$CONF" ]; then
  next=3390; suggestion=""
  for u in $AUTO $(human_users | grep -vx "${AUTO:-#}"); do suggestion+="$u $next"$'\n'; next=$((next+1)); done
  echo "Vorschlag für feste Ports:"; printf '%s' "$suggestion" | sed 's/^/  /'
  if ja "Vorschlag übernehmen?"; then printf '%s' "$suggestion" > "$CONF"; fi
fi
echo "Feste Ports (Bildschirmfreigabe):"
for u in $(human_users); do p=$(port_of "$u"); printf '  %-16s %s\n' "$u" "${p:-automatisch}"; done
echo "Weitere Benutzer oder Ports ändern: Benutzer eingeben, leer lassen = fertig."
while read -rp "Benutzer: " u && [ -n "$u" ]; do
  id "$u" >/dev/null 2>&1 || { echo "  Benutzer $u gibt es nicht."; continue; }
  read -rp "Port für $u (1024–65535, leer = automatisch): " p
  sed -i "/^$u /d" "$CONF"
  if [ -z "$p" ]; then echo "  $u: automatisch"; continue; fi
  if ! [[ $p =~ ^[0-9]+$ ]] || (( p < 1024 || p > 65535 )) || (( p == 3389 )); then
    echo "  Ungültig (3389 ist für den Anmeldebildschirm reserviert)."; continue
  fi
  if awk -v p="$p" '$2==p {f=1} END{exit !f}' "$CONF"; then echo "  Port $p ist schon vergeben."; continue; fi
  echo "$u $p" >> "$CONF"; echo "  $u: Port $p"
done

# --- 4. Zugangsdaten der Bildschirmfreigabe für angemeldete Benutzer -----------------------------
echo
for u in $(human_users); do
  logged_in "$u" || { echo "$u ist nicht angemeldet: Zugangsdaten später in dessen Sitzung setzen (Einstellungen > System > Remotedesktop) oder dieses Skript erneut starten, während $u angemeldet ist."; continue; }
  ja "Zugangsdaten für die Bildschirmfreigabe von $u setzen oder ändern?" || { as_user "$u" systemctl --user restart sesam-rdp-user.service gnome-remote-desktop.service || true; continue; }
  locked=$(as_user "$u" busctl --user get-property org.freedesktop.secrets /org/freedesktop/secrets/collection/login \
           org.freedesktop.Secret.Collection Locked 2>/dev/null || true)
  if [ "$locked" = "b true" ]; then
    if [ "$u" != "$AUTO" ]; then echo "  Schlüsselbund von $u ist gesperrt – $u bitte einmal am Bildschirm anmelden, dann erneut starten."; continue; fi
    echo "  $u meldet sich automatisch an, deshalb ist der Schlüsselbund gesperrt und die Freigabe kann die Zugangsdaten nicht lesen."
    echo "  Lösung: Schlüsselbund ohne Passwort (gespeicherte Passwörter von $u liegen dann unverschlüsselt auf der Platte)."
    ja "  Schlüsselbund von $u ohne Passwort speichern?" || continue
    read -rsp "  Bisheriges Passwort des Schlüsselbunds (meist das Ubuntu-Passwort von $u vor einer Änderung): " KP; echo
    # Passwort über stdin, nie als Argument (sonst landet es im Protokoll)
    printf '%s\n' "$KP" | as_user "$u" python3 -c '
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
    sys.exit("  Schlüsselbund nicht geändert (Passwort falsch?): " + e.message)
print("  ✓ Schlüsselbund ist jetzt ohne Passwort.")
' || continue
  fi
  echo "  Benutzername und Passwort für die Bildschirmfreigabe von $u eingeben (Tipp: Ubuntu-Benutzer und -Passwort von $u):"
  as_user "$u" grdctl rdp set-credentials
  as_user "$u" systemctl --user restart sesam-rdp-user.service gnome-remote-desktop.service || true
  echo "  ✓ Zugangsdaten für $u gesetzt."
done

# --- 5. Virtueller Monitor (Betrieb ohne Bildschirm) ---------------------------------------------
echo
if ja "Virtuellen Monitor einrichten (PC läuft auch mit ausgeschaltetem oder abgestecktem Bildschirm)?"; then
  mapfile -t CON < <(for c in /sys/class/drm/card*-*; do [ "$(cat "$c/status" 2>/dev/null)" = connected ] && basename "$c"; done)
  if [ ${#CON[@]} -eq 0 ]; then
    echo "  Kein Bildschirm angeschlossen. Bitte einmal anschliessen und das Skript erneut starten."
  else
    c=${CON[0]}
    if [ ${#CON[@]} -gt 1 ]; then echo "  Angeschlossen: ${CON[*]}"; read -rp "  Welcher Ausgang? [${CON[0]}] " a; c=${a:-$c}; fi
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
    update-initramfs -u >/dev/null 2>&1
    update-grub >/dev/null 2>&1
    echo "  ✓ Virtueller Monitor auf $name eingerichtet, wirkt nach dem nächsten Neustart."
  fi
fi

# --- 6. Firewall ---------------------------------------------------------------------------------
if command -v ufw >/dev/null && ufw status | head -1 | grep -qE ': (aktiv|active)$'; then
  echo
  if ja "Firewall ist aktiv. Port 3389 und die festen Ports freigeben?"; then
    ufw allow 3389/tcp >/dev/null
    while read -r _ p; do [ -n "$p" ] && ufw allow "$p/tcp" >/dev/null; done < "$CONF"
    echo "✓ Ports in der Firewall freigegeben."
  fi
fi

# --- Übersicht -----------------------------------------------------------------------------------
GRDHOME=$(getent passwd gnome-remote-desktop | cut -d: -f6)
echo
echo "================ Übersicht für Sesam ($(hostname), $(hostname -I | awk '{print $1}')) ================"
printf '  %-6s %-40s %s\n' 3389 "Anmeldebildschirm, beliebiger Benutzer" "$(fingerprint "$GRDHOME/.local/share/gnome-remote-desktop/certificates/rdp-tls.crt")"
while read -r u p; do
  [ -n "$u" ] || continue
  printf '  %-6s %-40s %s\n' "$p" "Sitzung von $u übernehmen" \
    "$(fingerprint "$(getent passwd "$u" | cut -d: -f6)/.local/share/gnome-remote-desktop/certificates/rdp-tls.crt")"
done < "$CONF"
echo "  Die letzte Spalte ist der Zertifikats-Fingerabdruck, den Sesam beim ersten Verbinden anzeigt."
echo "  Die Bildschirmfreigabe eines Benutzers läuft nur, solange er am Bildschirm angemeldet ist."
