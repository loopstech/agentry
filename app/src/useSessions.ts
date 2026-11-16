import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { Config, IslandLayout, Session, UiResponse } from "./types";

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

/** Bring the session's terminal tab or editor window forward. Resolves to a
 *  short description, rejects with a readable reason. */
export const jumpToSession = (id: string) => invoke<string>("jump_to_session", { id });

export const setPanelOpen = (open: boolean) => invoke<void>("set_panel_open", { open }).catch(() => {});

/** The user's settings, kept live: changes from any window (or from
 *  `agentry config`) arrive as events. */
export function useConfig() {
  const [config, setConfig] = useState<Config | null>(null);
  useEffect(() => {
    invoke<Config>("get_config").then(setConfig).catch(() => {});
    const sub = listen<Config>("agentry://config", (e) => setConfig(e.payload));
    return () => {
      sub.then((off) => off());
    };
  }, []);
  const setMuted = (muted: boolean) =>
    invoke<Config>("set_muted", { muted }).then(setConfig).catch(() => {});
  const save = (next: Config) =>
    invoke<Config>("save_config", { config: next }).then((c) => {
      setConfig(c);
      return c;
    });
  return { config, setMuted, save };
}

/** Whether the island is attached to a notch, kept live. */
export function useLayout() {
  const [layout, setLayout] = useState<IslandLayout>({ notch: null });
  useEffect(() => {
    invoke<IslandLayout>("get_layout").then(setLayout).catch(() => {});
    const sub = listen<IslandLayout>("agentry://layout", (e) => setLayout(e.payload));
    return () => {
      sub.then((off) => off());
    };
  }, []);
  return layout;
}

export const openSettings = () => invoke<void>("open_settings").catch(() => {});

export interface HooksStatus {
  settingsFile: string;
  installed: number;
  expected: number;
  missingPrograms: string[];
  hookProgram: string | null;
  claudeVersion: string | null;
}
export const hooksStatus = () => invoke<HooksStatus>("hooks_status");
export const installHooks = () => invoke<string>("install_hooks");
export const uninstallHooks = () => invoke<string>("uninstall_hooks");
export const getLaunchAtLogin = () => invoke<boolean>("get_launch_at_login");
export const setLaunchAtLogin = (enabled: boolean) => invoke<boolean>("set_launch_at_login", { enabled });
export const checkUpdates = () => invoke<string>("check_updates");
export const restartApp = () => invoke<void>("restart_app");
export const installCli = () => invoke<string>("install_cli");
export const finishWelcome = () => invoke<void>("finish_welcome");

export const playSound = (cue: "needs_you" | "finished") => invoke<void>("play_sound", { cue }).catch(() => {});

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
