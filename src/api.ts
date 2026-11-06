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

export const api = {
  list: () => invoke<Connection[]>("library_list"),
  /** password/sshKey: undefined = unverändert, "" = löschen */
  save: (connection: Connection, password?: string, sshKey?: string) =>
    invoke<Connection>("library_save", { connection, password, sshKey }),
  remove: (id: string) => invoke<void>("library_delete", { id }),
  shellOpen: (id: string, cols: number, rows: number, onEvent: (e: ShellEvent) => void) => {
    const ch = new Channel<ShellEvent>();
    ch.onmessage = onEvent;
    return invoke<string>("shell_open", { id, cols, rows, onEvent: ch });
  },
  shellWrite: (sid: string, data: Uint8Array) => invoke<void>("shell_write", { sid, data: Array.from(data) }),
  shellResize: (sid: string, cols: number, rows: number) => invoke<void>("shell_resize", { sid, cols, rows }),
  shellClose: (sid: string) => invoke<void>("shell_close", { sid }),
};
