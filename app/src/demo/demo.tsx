// The demo page: the real UI with Tauri's IPC mocked and made-up sessions.
// Used to take the screenshots in docs/images (scripts/screenshots.mjs).
// Only served by `pnpm dev`; it isn't part of the app's build.
//
//   http://localhost:1420/demo.html?theme=crew&scene=panel
//   http://localhost:1420/demo.html?view=settings&theme=paper
//
// scene: panel | pill | question | warning | transcript | empty
// open:  index of a row whose recent steps to show (transcript scene)

import React from "react";
import ReactDOM from "react-dom/client";
import { emit } from "@tauri-apps/api/event";
import { mockIPC, mockWindows } from "@tauri-apps/api/mocks";
import "@fontsource/geist-sans/400.css";
import "@fontsource/geist-sans/500.css";
import "@fontsource/geist-sans/600.css";
import "@fontsource/geist-sans/700.css";
import "@fontsource/geist-mono/400.css";
import "@fontsource/bricolage-grotesque/400.css";
import "@fontsource/bricolage-grotesque/600.css";
import "@fontsource/bricolage-grotesque/800.css";
import "../components/components.css";
import "./demo.css";
import { Island } from "../components/Island";
import { Settings } from "../settings/Settings";
import type { ThemeId } from "../types";
import { SCENES, demoConfig, demoTranscript } from "./data";

const params = new URLSearchParams(window.location.search);
const theme = (params.get("theme") ?? "mono") as ThemeId;
const scene = params.get("scene") ?? "panel";
const view = params.get("view") ?? "island";
const now = Date.now();
const sessions = (SCENES[scene] ?? SCENES.panel)(now);
let config = demoConfig(theme);

mockWindows(view === "settings" ? "settings" : "main");
mockIPC(
  (cmd, args) => {
    const a = (args ?? {}) as Record<string, unknown>;
    switch (cmd) {
      case "get_sessions":
        return sessions;
      case "server_info":
        return { port: 4321, error: null };
      case "get_config":
        return config;
      case "save_config":
        config = a.config as typeof config;
        return config;
      case "set_muted":
        config = { ...config, muted: Boolean(a.muted) };
        return config;
      case "get_layout":
        return { notch: null };
      case "set_panel_size":
        return Math.min(Number(a.height ?? 900), 900);
      case "get_transcript":
        return demoTranscript(now);
      case "hooks_status":
        return {
          settingsFile: "~/.claude/settings.json",
          installed: 11,
          expected: 11,
          missingPrograms: [],
          hookProgram: "/Applications/Agentry.app/Contents/MacOS/agentry-hook",
          claudeVersion: "2.1.325",
        };
      case "get_launch_at_login":
        return true;
      case "respond_request":
      case "jump_to_session":
      case "send_message":
        return "OK";
      default:
        return null;
    }
  },
  { shouldMockEvents: true },
);

document.body.dataset.theme = theme;
document.body.dataset.view = view;
if (view === "settings") window.location.hash = "#settings";

ReactDOM.createRoot(document.getElementById("stage")!).render(
  <React.StrictMode>{view === "settings" ? <Settings /> : <Island />}</React.StrictMode>,
);

// Open the panel the way the app does when a request arrives, then open a
// row's recent steps if asked. `data-ready` tells the screenshot script.
setTimeout(async () => {
  if (view === "island" && scene !== "pill") {
    await emit("agentry://request", { sessionId: sessions[0]?.id ?? "", openPanel: true });
  }
  const open = params.get("open");
  if (open !== null) {
    await new Promise((r) => setTimeout(r, 300));
    document.querySelectorAll<HTMLButtonElement>(".row-text")[Number(open)]?.click();
  }
  setTimeout(() => (document.body.dataset.ready = "1"), 900);
}, 300);
