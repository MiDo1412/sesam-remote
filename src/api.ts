import { invoke, Channel } from "@tauri-apps/api/core";

export interface Connection {
  id: string;
  name: string;
  group: string;
  address: string;
  vncPort: number | null;
  sshPort: number | null;
  rdpPort: number | null;
  rdpDomain: string;
  /** Eigener RDP-Benutzer; leer = Benutzer der Verbindung */
  rdpUser: string;
  /** "" Windows/anderer Server, "login" GNOME-Anmeldebildschirm, "session" laufende Sitzung übernehmen */
  rdpTarget: "" | "login" | "session";
  rdpSessionUser: string;
  desktopProtocol: "vnc" | "rdp";
  user: string;
  defaultView: "desktop" | "shell" | "files";
  orientation: "landscape" | "portrait";
  sharedClipboard: boolean;
  playAudio: boolean;
  /** Zertifikat nicht prüfen (unsicher, nur für Tests). */
  skipCertCheck: boolean;
  os: "" | "ubuntu" | "linux" | "macos" | "windows" | "raspberry";
  customThumb: boolean;
  credentialId: string | null;
  /** Gespeicherte Zugangsdaten nur für RDP, VNC bzw. das RD-Gateway */
  rdpCredentialId: string | null;
  vncCredentialId: string | null;
  gatewayCredentialId: string | null;
  /** RDP nur mit Kerberos. */
  rdpNoNtlm: boolean;
  /** KDC oder KDC-Proxy-URL; leer = per DNS. */
  rdpKdc: string;
  /** RD-Gateway host[:port]; leer = direkt. */
  rdpGateway: string;
  /** Leer = gleiche Zugangsdaten wie die Verbindung. */
  rdpGatewayUser: string;
  sharedFolder: string;
  fitToWindow: boolean;
  hasPassword: boolean;
  hasVncPassword: boolean;
  hasRdpPassword: boolean;
  hasGatewayPassword: boolean;
  hasKey: boolean;
  /** Dateiübertragung: SFTP, sonst SCP ("auto") oder fest. */
  fileTransfer: "auto" | "sftp" | "scp";
  /** null = FTP aus */
  ftpPort: number | null;
  /** FTPS explizit (AUTH TLS), implizit oder unverschlüsselt */
  ftpSecurity: "explicit" | "implicit" | "none";
  ftpActive: boolean;
  /** Bei SSH und FTP: womit «Dateien» verbindet */
  filesVia: "ssh" | "ftp";
  /** Veraltete SSH-Algorithmen (SHA-1, CBC, 3DES, ssh-rsa) zusätzlich anbieten. */
  sshLegacy: boolean;
  /** Sekunden, 0 = aus */
  sshKeepalive: number;
  sshCompression: boolean;
  /** Portweiterleitungen der Shell: «L8080 localhost:80», «R9000 localhost:3000», «D1080» */
  forwards: string[];
  /** Sprungserver: ID einer anderen Verbindung, "" = direkt */
  jumpHost: string;
  /** Zuerst mit dem SSH-Agenten anmelden (Pageant / Windows-OpenSSH-Agent). */
  sshAgent: boolean;
  /** Agent an den Server weiterreichen (nur Shell, nur mit sshAgent). */
  agentForward: boolean;
  lastUsed: string | null;
  /** MAC-Adresse für Wake-on-LAN (selbst ermittelt oder eingetragen). */
  mac: string;
  /** Desktop durch einen SSH-Tunnel. */
  sshTunnel: boolean;
}

export interface Cred {
  id: string;
  name: string;
  user: string;
  hasPassword: boolean;
  hasKey: boolean;
}

/** Gruppe der Seitenleiste (Reihenfolge, Farbe); Mitglieder über `Connection.group`. */
export interface Group {
  name: string;
  /** "" oder eine Vorgabe aus GROUP_COLORS (groups.ts) */
  color: string;
}

export type ShellEvent = { kind: "data"; data: number[] } | { kind: "closed"; reason: string };

/** Präfix eines Fehlers bei unbekanntem Host: `HOSTKEY_UNKNOWN:<Algorithmus>|<SHA256-Fingerabdruck>` */
export const HOSTKEY_UNKNOWN = "HOSTKEY_UNKNOWN:";
/** VNC über TLS: `CERT_UNKNOWN:<SHA256>` bzw. `CERT_CHANGED:<SHA256>` */
/** Anzeigeskalierung dieses Bildschirms in Prozent (Windows «Skalierung», macOS Retina), für RDP. */
const displayScale = () => Math.min(500, Math.max(100, Math.round((devicePixelRatio || 1) * 100)));
export const CERT_UNKNOWN = "CERT_UNKNOWN:";
export const CERT_CHANGED = "CERT_CHANGED:";

export interface Entry {
  name: string;
  dir: boolean;
  size: number;
  /** Sekunden seit 1970 */
  modified: number | null;
  /** Unix-Rechte, nur bei SFTP */
  perm: number | null;
}
export interface Listing {
  path: string;
  /** null = oberste Ebene; "" = Laufwerksliste (Windows) */
  parent: string | null;
  entries: Entry[];
}
export type Progress =
  | { state: "running"; done: number; total: number; file: string }
  | { state: "done"; target: string }
  | { state: "failed"; error: string };

/** Womit «Desktop» verbindet: RDP, wenn nur RDP an ist oder bei beiden RDP gewählt ist. */
export const desktopProtocol = (c: Connection): "vnc" | "rdp" | null =>
  c.rdpPort && (!c.vncPort || c.desktopProtocol === "rdp") ? "rdp" : c.vncPort ? "vnc" : null;

/** Womit «Dateien» verbindet; wie `Connection::files_over_ftp` in library.rs. */
export const filesProtocol = (c: Connection): "ssh" | "ftp" | null =>
  c.ftpPort && (!c.sshPort || c.filesVia === "ftp") ? "ftp" : c.sshPort ? "ssh" : null;

/** Kurzname des FTP-Dienstes für Anzeigen. */
export const ftpLabel = (c: Connection) => (c.ftpSecurity === "none" ? "FTP (unverschlüsselt)" : "FTPS");

/** Kanal-Nachrichten mit Bilddaten: kleine kommen als ArrayBuffer, grosse (über den IPC-Abruf) teils als
 *  Bytefeld (Uint8Array oder number[]). Ohne Umwandlung warf `new DataView` und kein Bild erschien. */
export function asArrayBuffer(m: unknown): ArrayBuffer {
  if (m instanceof ArrayBuffer) return m;
  if (ArrayBuffer.isView(m)) return m.buffer.slice(m.byteOffset, m.byteOffset + m.byteLength) as ArrayBuffer;
  if (Array.isArray(m)) return new Uint8Array(m).buffer;
  throw new Error(`Unerwartete Bilddaten: ${Object.prototype.toString.call(m)}`);
}

// Selbstprüfung im Entwicklungsbuild.
if (import.meta.env.DEV) {
  const v = new Uint8Array([9, 1, 2, 3]).subarray(1);
  console.assert(new Uint8Array(asArrayBuffer(v))[0] === 1 && asArrayBuffer([0, 5]).byteLength === 2 && asArrayBuffer(new ArrayBuffer(3)).byteLength === 3, "asArrayBuffer");
}

export const api = {
  list: () => invoke<Connection[]>("library_list"),
  /** Geheimnisse: undefined = unverändert, "" = löschen */
  save: (connection: Connection, secrets: { password?: string; vncPassword?: string; rdpPassword?: string; gatewayPassword?: string; sshKey?: string }) =>
    invoke<Connection>("library_save", { connection, ...secrets }),
  remove: (id: string) => invoke<void>("library_delete", { id }),
  rdpCheck: (id: string, trust?: string, remember?: boolean) => invoke<void>("rdp_check", { id, trust, remember }),
  rdpPorts: (host: string, port: number, user: string, password: string | undefined, id: string | undefined, trust: string | null) =>
    invoke<[string, number][]>("rdp_ports", { host, port, user, password, id, trust }),
  groupsList: () => invoke<Group[]>("groups_list"),
  groupsSave: (groups: Group[]) => invoke<void>("groups_save", { groups }),
  groupRename: (from: string, to: string) => invoke<void>("group_rename", { from, to }),
  groupDelete: (name: string) => invoke<void>("group_delete", { name }),
  credsList: () => invoke<[Cred, number][]>("creds_list"),
  credSave: (cred: Cred, password?: string, sshKey?: string) => invoke<Cred>("cred_save", { cred, password, sshKey }),
  credDelete: (id: string) => invoke<void>("cred_delete", { id }),
  credFromConnection: (id: string, credId: string, name: string) => invoke<Cred>("cred_from_connection", { id, credId, name }),
  quickRegister: (connection: Connection, password?: string, keyFile?: string) => invoke<string>("quick_register", { connection, password, keyFile }),
  shellOpen: (id: string, cols: number, rows: number, trust: string | null, onEvent: (e: ShellEvent) => void) => {
    const ch = new Channel<ShellEvent>();
    ch.onmessage = onEvent;
    return invoke<string>("shell_open", { id, cols, rows, trust, onEvent: ch });
  },
  /** Laufende Shell an dieses Fenster hängen (Tab abkoppeln/andocken). */
  shellAttach: (sid: string, onEvent: (e: ShellEvent) => void) => {
    const ch = new Channel<ShellEvent>();
    ch.onmessage = onEvent;
    return invoke<void>("shell_attach", { sid, onEvent: ch });
  },
  desktopAttach: (sid: string, onFrame: (buf: ArrayBuffer) => void) => {
    const ch = new Channel<ArrayBuffer>();
    ch.onmessage = (m) => onFrame(asArrayBuffer(m));
    return invoke<void>("desktop_attach", { sid, onEvent: ch });
  },
  /** Lokales Terminal (PTY) mit einem Profil starten; Schreiben/Grösse/Schliessen wie bei der SSH-Shell. */
  /** Zwischenablage über das System statt navigator.clipboard (das fragt in WebView2 nach einer Berechtigung). */
  clipRead: () => invoke<string>("clip_read"),
  /** Erreichbarkeit (Latenz in ms, null = nicht erreichbar); merkt sich dabei die MAC-Adresse. */
  connReach: (id: string) => invoke<number | null>("conn_reach", { id }),
  connWake: (id: string) => invoke<void>("conn_wake", { id }),
  clipWrite: (text: string) => invoke<void>("clip_write", { text }),
  termOpen: (profile: { command: string; args: string[]; cwd?: string; env?: Record<string, string> }, cols: number, rows: number, onEvent: (e: ShellEvent) => void) => {
    const ch = new Channel<ShellEvent>();
    ch.onmessage = onEvent;
    return invoke<string>("term_open", { profile: { command: profile.command, args: profile.args, cwd: profile.cwd ?? null, env: profile.env ?? {} }, cols, rows, onEvent: ch });
  },
  shellWrite: (sid: string, data: Uint8Array) => invoke<void>("shell_write", { sid, data: Array.from(data) }),
  shellResize: (sid: string, cols: number, rows: number) => invoke<void>("shell_resize", { sid, cols, rows }),
  shellClose: (sid: string) => invoke<void>("shell_close", { sid }),
  /** Binärrahmen: 0 Rechteck, 1 Grösse, 3 Ende, 4 Glocke, 5 Hinweis, 6 Mauszeiger (siehe lib.rs `frame`). */
  /** remember: bestätigten Fingerabdruck speichern (sonst nur diese Verbindung). */
  vncOpen: (id: string, trust: string | null, remember: boolean, onFrame: (buf: ArrayBuffer) => void) => {
    const ch = new Channel<ArrayBuffer>();
    ch.onmessage = (m) => onFrame(asArrayBuffer(m));
    return invoke<string>("vnc_open", { id, trust, remember, onEvent: ch });
  },
  desktopAck: (sid: string) => invoke<void>("desktop_ack", { sid }),
  vncPointer: (sid: string, mask: number, x: number, y: number) => invoke<void>("vnc_pointer", { sid, mask, x, y }),
  vncKey: (sid: string, down: boolean, keysym: number) => invoke<void>("vnc_key", { sid, down, keysym }),
  vncSetSize: (sid: string, width: number, height: number) => invoke<boolean>("vnc_set_size", { sid, width, height }),
  vncClipboardSync: (sid: string) => invoke<void>("vnc_clipboard_sync", { sid }),
  vncClose: (sid: string) => invoke<void>("vnc_close", { sid }),
  rdpOpen: (id: string, trust: string | null, remember: boolean, width: number, height: number, onFrame: (buf: ArrayBuffer) => void) => {
    const ch = new Channel<ArrayBuffer>();
    ch.onmessage = (m) => onFrame(asArrayBuffer(m));
    return invoke<string>("rdp_open", { id, trust, remember, width, height, scale: displayScale(), onEvent: ch });
  },
  rdpPointer: (sid: string, x: number, y: number, buttons: number) => invoke<void>("rdp_pointer", { sid, x, y, buttons }),
  rdpWheel: (sid: string, vertical: boolean, units: number) => invoke<void>("rdp_wheel", { sid, vertical, units }),
  rdpKey: (sid: string, code: string, key: string, down: boolean, altgr: boolean) => invoke<void>("rdp_key", { sid, code, key, down, altgr }),
  rdpReleaseAll: (sid: string) => invoke<void>("rdp_release_all", { sid }),
  rdpSetMonitors: (sid: string, monitors: { left: number; top: number; width: number; height: number; primary: boolean }[]) =>
    invoke<void>("rdp_set_monitors", { sid, monitors }),
  rdpSetSize: (sid: string, width: number, height: number) => invoke<void>("rdp_set_size", { sid, width, height, scale: displayScale() }),
  rdpClose: (sid: string) => invoke<void>("rdp_close", { sid }),
  /** scp: Server ohne SFTP, Übertragung über SCP (eingeschränkt). */
  /** trust: SSH-Host-Schlüssel oder (FTPS) Zertifikat; remember: Zertifikat merken. */
  sftpOpen: (id: string, trust: string | null, remember = true) => invoke<{ sid: string; scp: boolean }>("sftp_open", { id, trust, remember }),
  sftpList: (sid: string, path: string) => invoke<Listing>("sftp_list", { sid, path }),
  sftpMkdir: (sid: string, dir: string, name: string) => invoke<string>("sftp_mkdir", { sid, dir, name }),
  sftpRename: (sid: string, path: string, name: string) => invoke<string>("sftp_rename", { sid, path, name }),
  sftpRemove: (sid: string, path: string) => invoke<void>("sftp_remove", { sid, path }),
  sftpClose: (sid: string) => invoke<void>("sftp_close", { sid }),
  sftpChmod: (sid: string, path: string, mode: number) => invoke<void>("sftp_chmod", { sid, path, mode }),
  sftpTouch: (sid: string, dir: string, name: string) => invoke<string>("sftp_touch", { sid, dir, name }),
  sftpOwner: (sid: string, path: string) => invoke<string>("sftp_owner", { sid, path }),
  sftpSetPerms: (sid: string, path: string, mode: number | null, owner: string | null, recursive: boolean, sudo: boolean, password?: string) =>
    invoke<void>("sftp_set_perms", { sid, path, mode, owner, recursive, sudo, password }),
  sftpTempdir: (sid: string) => invoke<string>("sftp_tempdir", { sid }),
  sftpSudoMove: (sid: string, from: string, destDir: string, replace: boolean, password?: string) => invoke<void>("sftp_sudo_move", { sid, from, destDir, replace, password }),
  /** `op`: remove, rename (arg = neuer Name), mkdir/touch (arg = Name), fetch (arg = temporärer Ordner). */
  sftpSudoOp: (sid: string, op: string, path: string, arg: string, password?: string) => invoke<void>("sftp_sudo_op", { sid, op, path, arg, password }),
  /** SHA-256 auf dem Server (sha256sum über SSH). */
  sftpSha256: (sid: string, path: string) => invoke<string>("sftp_sha256", { sid, path }),
  localSha256: (path: string) => invoke<string>("local_sha256", { path }),
  localMkdir: (dir: string, name: string) => invoke<string>("local_mkdir", { dir, name }),
  localRename: (path: string, name: string) => invoke<string>("local_rename", { path, name }),
  localList: (path: string) => invoke<Listing>("local_list", { path }),
  localHome: () => invoke<string>("local_home"),
  /** upload: lokal → Server; sonst Server → lokal. `replace`: gleichnamige Datei ersetzen (nur nach Rückfrage). Gibt die Übertragungs-ID zurück. */
  transferStart: (sid: string, upload: boolean, src: string, destDir: string, onProgress: (p: Progress) => void, replace = false) => {
    const ch = new Channel<Progress>();
    ch.onmessage = onProgress;
    return invoke<string>("transfer_start", { sid, upload, src, destDir, replace, onProgress: ch });
  },
  transferCancel: (tid: string) => invoke<void>("transfer_cancel", { tid }),
};
