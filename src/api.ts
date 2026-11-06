import { invoke, Channel } from "@tauri-apps/api/core";

export interface Connection {
  id: string;
  name: string;
  group: string;
  address: string;
  vncPort: number | null;
  sshPort: number | null;
  user: string;
  defaultView: "desktop" | "shell" | "files";
  orientation: "landscape" | "portrait";
  sharedClipboard: boolean;
  fitToWindow: boolean;
  hasPassword: boolean;
  hasKey: boolean;
  lastUsed: string | null;
}

export type ShellEvent = { kind: "data"; data: number[] } | { kind: "closed"; reason: string };

/** Präfix eines Fehlers bei unbekanntem Host: `HOSTKEY_UNKNOWN:<Algorithmus>|<SHA256-Fingerabdruck>` */
export const HOSTKEY_UNKNOWN = "HOSTKEY_UNKNOWN:";
/** VNC über TLS: `CERT_UNKNOWN:<SHA256>` bzw. `CERT_CHANGED:<SHA256>` */
export const CERT_UNKNOWN = "CERT_UNKNOWN:";
export const CERT_CHANGED = "CERT_CHANGED:";

export const api = {
  list: () => invoke<Connection[]>("library_list"),
  /** password/sshKey: undefined = unverändert, "" = löschen */
  save: (connection: Connection, password?: string, sshKey?: string) =>
    invoke<Connection>("library_save", { connection, password, sshKey }),
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
  vncOpen: (id: string, trust: string | null, onFrame: (buf: ArrayBuffer) => void) => {
    const ch = new Channel<ArrayBuffer>();
    ch.onmessage = onFrame;
    return invoke<string>("vnc_open", { id, trust, onEvent: ch });
  },
  vncPointer: (sid: string, mask: number, x: number, y: number) => invoke<void>("vnc_pointer", { sid, mask, x, y }),
  vncKey: (sid: string, down: boolean, keysym: number) => invoke<void>("vnc_key", { sid, down, keysym }),
  vncSetSize: (sid: string, width: number, height: number) => invoke<boolean>("vnc_set_size", { sid, width, height }),
  vncClipboardSync: (sid: string) => invoke<void>("vnc_clipboard_sync", { sid }),
  vncClose: (sid: string) => invoke<void>("vnc_close", { sid }),
};
