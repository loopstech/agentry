import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { Session } from "./types";

export interface ServerInfo {
  port: number | null;
  error: string | null;
}

/** Live list of sessions from the app's core, in display order. */
export function useSessions() {
  const [sessions, setSessions] = useState<Session[]>([]);
  const [server, setServer] = useState<ServerInfo>({ port: null, error: null });

  useEffect(() => {
    const refresh = () => invoke<Session[]>("get_sessions").then(setSessions).catch(() => {});
    invoke<ServerInfo>("server_info").then(setServer).catch(() => {});
    refresh();
    const subs = [
      listen<Session[]>("agentry://sessions", (e) => setSessions(e.payload)),
      listen<ServerInfo>("agentry://server", (e) => {
        setServer(e.payload);
        refresh();
      }),
    ];
    return () => subs.forEach((p) => p.then((off) => off()));
  }, []);

  return { sessions, server };
}

export const dismissSession = (id: string) => invoke<boolean>("dismiss_session", { id });

/** Current time, re-rendered every second while `active`. */
export function useNow(active: boolean) {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    if (!active) return;
    const t = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(t);
  }, [active]);
  return now;
}
