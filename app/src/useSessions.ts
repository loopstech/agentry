import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { Config, Session, UiResponse } from "./types";

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

/** Answer a pending request. Rejects with a readable message (e.g. it was
 *  already answered in the terminal). */
export const respondRequest = (requestId: string, response: UiResponse) =>
  invoke<void>("respond_request", { requestId, response });

export const setPanelOpen = (open: boolean) => invoke<void>("set_panel_open", { open }).catch(() => {});

/** The user's settings, plus a mute toggle. */
export function useConfig() {
  const [config, setConfig] = useState<Config | null>(null);
  useEffect(() => {
    invoke<Config>("get_config").then(setConfig).catch(() => {});
  }, []);
  const setMuted = (muted: boolean) =>
    invoke<Config>("set_muted", { muted }).then(setConfig).catch(() => {});
  return { config, setMuted };
}

/** Calls `onRequest` when a new request arrives in the widget. */
export function useRequestArrivals(onRequest: (r: { sessionId: string; openPanel: boolean }) => void) {
  useEffect(() => {
    const sub = listen<{ sessionId: string; openPanel: boolean }>("agentry://request", (e) => onRequest(e.payload));
    return () => {
      sub.then((off) => off());
    };
  }, [onRequest]);
}

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
