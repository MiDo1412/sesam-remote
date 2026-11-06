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
  desktopProtocol: "vnc" | "rdp";
  user: string;
  defaultView: "desktop" | "shell" | "files";
  orientation: "landscape" | "portrait";
  sharedClipboard: boolean;
  playAudio: boolean;
  /** Zertifikat nicht prüfen (unsicher, nur für Tests). */
  skipCertCheck: boolean;
  sharedFolder: string;
  fitToWindow: boolean;
  hasPassword: boolean;
  hasVncPassword: boolean;
  hasRdpPassword: boolean;
  hasKey: boolean;
  /** Dateiübertragung: SFTP, sonst SCP ("auto") oder fest. */
  fileTransfer: "auto" | "sftp" | "scp";
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
}

export type ShellEvent = { kind: "data"; data: number[] } | { kind: "closed"; reason: string };

/** Präfix eines Fehlers bei unbekanntem Host: `HOSTKEY_UNKNOWN:<Algorithmus>|<SHA256-Fingerabdruck>` */
export const HOSTKEY_UNKNOWN = "HOSTKEY_UNKNOWN:";
/** VNC über TLS: `CERT_UNKNOWN:<SHA256>` bzw. `CERT_CHANGED:<SHA256>` */
export const CERT_UNKNOWN = "CERT_UNKNOWN:";
export const CERT_CHANGED = "CERT_CHANGED:";

export interface Entry {
  name: string;
  dir: boolean;
  size: number;
  /** Sekunden seit 1970 */
  modified: number | null;
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

export const api = {
  list: () => invoke<Connection[]>("library_list"),
  /** Geheimnisse: undefined = unverändert, "" = löschen */
  save: (connection: Connection, secrets: { password?: string; vncPassword?: string; rdpPassword?: string; sshKey?: string }) =>
    invoke<Connection>("library_save", { connection, ...secrets }),
  remove: (id: string) => invoke<void>("library_delete", { id }),
  shellOpen: (id: string, cols: number, rows: number, trust: string | null, onEvent: (e: ShellEvent) => void) => {
    const ch = new Channel<ShellEvent>();
    ch.onmessage = onEvent;
    return invoke<string>("shell_open", { id, cols, rows, trust, onEvent: ch });
  },
  shellWrite: (sid: string, data: Uint8Array) => invoke<void>("shell_write", { sid, data: Array.from(data) }),
  shellResize: (sid: string, cols: number, rows: number) => invoke<void>("shell_resize", { sid, cols, rows }),
  shellClose: (sid: string) => invoke<void>("shell_close", { sid }),
  /** Binärrahmen: 0 Rechteck, 1 Grösse, 3 Ende, 4 Glocke, 5 Hinweis, 6 Mauszeiger (siehe lib.rs `frame`). */
  /** remember: bestätigten Fingerabdruck speichern (sonst nur diese Verbindung). */
  vncOpen: (id: string, trust: string | null, remember: boolean, onFrame: (buf: ArrayBuffer) => void) => {
    const ch = new Channel<ArrayBuffer>();
    ch.onmessage = onFrame;
    return invoke<string>("vnc_open", { id, trust, remember, onEvent: ch });
  },
  vncPointer: (sid: string, mask: number, x: number, y: number) => invoke<void>("vnc_pointer", { sid, mask, x, y }),
  vncKey: (sid: string, down: boolean, keysym: number) => invoke<void>("vnc_key", { sid, down, keysym }),
  vncSetSize: (sid: string, width: number, height: number) => invoke<boolean>("vnc_set_size", { sid, width, height }),
  vncClipboardSync: (sid: string) => invoke<void>("vnc_clipboard_sync", { sid }),
  vncClose: (sid: string) => invoke<void>("vnc_close", { sid }),
  rdpOpen: (id: string, trust: string | null, remember: boolean, width: number, height: number, onFrame: (buf: ArrayBuffer) => void) => {
    const ch = new Channel<ArrayBuffer>();
    ch.onmessage = onFrame;
    return invoke<string>("rdp_open", { id, trust, remember, width, height, onEvent: ch });
  },
  rdpPointer: (sid: string, x: number, y: number, buttons: number) => invoke<void>("rdp_pointer", { sid, x, y, buttons }),
  rdpWheel: (sid: string, vertical: boolean, units: number) => invoke<void>("rdp_wheel", { sid, vertical, units }),
  rdpKey: (sid: string, code: string, key: string, down: boolean, altgr: boolean) => invoke<void>("rdp_key", { sid, code, key, down, altgr }),
  rdpReleaseAll: (sid: string) => invoke<void>("rdp_release_all", { sid }),
  rdpSetMonitors: (sid: string, monitors: { left: number; top: number; width: number; height: number; primary: boolean }[]) =>
    invoke<void>("rdp_set_monitors", { sid, monitors }),
  rdpSetSize: (sid: string, width: number, height: number) => invoke<void>("rdp_set_size", { sid, width, height }),
  rdpClose: (sid: string) => invoke<void>("rdp_close", { sid }),
  /** scp: Server ohne SFTP, Übertragung über SCP (eingeschränkt). */
  sftpOpen: (id: string, trust: string | null) => invoke<{ sid: string; scp: boolean }>("sftp_open", { id, trust }),
  sftpList: (sid: string, path: string) => invoke<Listing>("sftp_list", { sid, path }),
  sftpMkdir: (sid: string, dir: string, name: string) => invoke<string>("sftp_mkdir", { sid, dir, name }),
  sftpRename: (sid: string, path: string, name: string) => invoke<string>("sftp_rename", { sid, path, name }),
  sftpRemove: (sid: string, path: string) => invoke<void>("sftp_remove", { sid, path }),
  sftpClose: (sid: string) => invoke<void>("sftp_close", { sid }),
  localList: (path: string) => invoke<Listing>("local_list", { path }),
  localHome: () => invoke<string>("local_home"),
  /** upload: lokal → Server; sonst Server → lokal. Gibt die Übertragungs-ID zurück. */
  transferStart: (sid: string, upload: boolean, src: string, destDir: string, onProgress: (p: Progress) => void) => {
    const ch = new Channel<Progress>();
    ch.onmessage = onProgress;
    return invoke<string>("transfer_start", { sid, upload, src, destDir, onProgress: ch });
  },
  transferCancel: (tid: string) => invoke<void>("transfer_cancel", { tid }),
};
