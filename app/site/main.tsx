// The website. The widget in the hero is the app's real UI with Tauri's IPC
// mocked: a short script plays Claude Code sessions into it, and the
// visitor's clicks (Allow, Deny, answers) change what happens next.

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
import "../src/components/components.css";
import "./site.css";
import { Island } from "../src/components/Island";
import { THEMES } from "../src/themes";
import type { Session, ThemeId, UiResponse } from "../src/types";
import { bash, demoConfig, demoTranscript, question, ready, working } from "../src/demo/data";
import shotMono from "../../docs/images/panel-mono.png";
import shotCrew from "../../docs/images/theme-crew.png";
import shotTerminal from "../../docs/images/theme-terminal.png";
import shotPaper from "../../docs/images/theme-paper.png";
import shotPixel from "../../docs/images/theme-pixel.png";
import shotGlass from "../../docs/images/theme-glass.png";

const reducedMotion = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

// ---------- The sessions, and how they change ----------

let config = demoConfig("mono");
let sessions: Session[] = [];
/** Resolves when the visitor answers the current request. */
let answered: ((r: UiResponse) => void) | null = null;

const publish = () => emit("agentry://sessions", sessions);
const update = (id: string, change: Partial<Session>) => {
  sessions = sessions.map((s) => (s.id === id ? { ...s, ...change, updatedAt: Date.now() } : s));
  publish();
};
const isOpen = () => document.querySelector("#widget .island-shape.is-expanded") !== null;
const close = () => {
  if (isOpen()) emit("agentry://toggle-panel");
};
const hint = (text: string) => {
  const el = document.getElementById("hint");
  if (el) el.textContent = text;
};

mockWindows("main");
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
      case "set_muted":
        config = { ...config, muted: Boolean(a.muted) };
        emit("agentry://config", config);
        return config;
      case "get_layout":
        return { notch: { width: 190, height: 34 } };
      case "set_panel_size":
        return 620;
      case "get_transcript":
        return demoTranscript(Date.now());
      case "respond_request": {
        const done = answered;
        answered = null;
        done?.(a.response as UiResponse);
        return null;
      }
      case "dismiss_session":
        sessions = sessions.filter((s) => s.id !== a.id);
        publish();
        return true;
      case "set_talk":
        update(String(a.id), { talk: Boolean(a.talk) });
        return true;
      case "jump_to_session":
        return "On your Mac, this brings the session's terminal tab forward";
      case "send_message":
        return "On your Mac, this types your message into Claude's terminal";
      default:
        return null;
    }
  },
  { shouldMockEvents: true },
);

/** Show a request and wait for the visitor's answer. */
async function ask(session: Session): Promise<UiResponse> {
  const fresh = { ...session, updatedAt: Date.now() };
  sessions = [fresh, ...sessions.filter((s) => s.id !== session.id)];
  const reply = new Promise<UiResponse>((resolve) => (answered = resolve));
  await publish();
  await emit("agentry://request", { sessionId: session.id, openPanel: !reducedMotion });
  return reply;
}

async function play() {
  const now = Date.now();
  sessions = [
    { ...bash(now), status: "working", subtitle: "Reading auth/login.spec.ts", pending: [], needsSince: null },
    working(now),
    ready(now, false),
  ];
  await publish();
  if (!reducedMotion) await sleep(2200);

  for (;;) {
    // A permission request.
    const login = bash(Date.now());
    hint("The widget up there is the real one, running in this page. When Claude asks, press Allow.");
    const r1 = await ask(login);
    const allowed = r1.action === "allow" || r1.action === "always_allow";
    update(login.id, {
      pending: [],
      needsSince: null,
      status: "working",
      subtitle: allowed ? "Running pnpm test auth/login.spec.ts" : "Trying a different approach",
    });
    hint(
      allowed
        ? "Allowed. On your Mac, the command runs in Claude's terminal right now."
        : r1.action === "terminal"
          ? "Handed back. On your Mac, Claude's own prompt takes over in the terminal."
          : "Denied. Claude reads your note and tries something else.",
    );
    await sleep(1400);
    close();
    await sleep(2600);
    update(login.id, {
      status: "idle",
      subtitle: "Finished — ready for you",
      said: allowed ? "The test was flaky because of a shared clock. Fixed it; 5 runs in a row pass." : "Done another way.",
    });
    update("s-dark", { progress: { done: 4, total: 5, label: "Checking contrast" }, subtitle: "Checking contrast" });
    await sleep(reducedMotion ? 1500 : 5000);

    // A question.
    const docs = question(Date.now());
    hint("Now Claude has a question for you. Pick an answer in the panel.");
    const r2 = await ask(docs);
    const choice = r2.action === "answer" ? Object.values(r2.answers)[0] : null;
    update(docs.id, {
      pending: [],
      needsSince: null,
      status: "working",
      subtitle: choice ? `Writing the notes for ${choice.toLowerCase()}` : "Writing the notes",
    });
    hint(choice ? `Answered “${choice}”. Claude carries on with your choice.` : "Claude carries on.");
    await sleep(1400);
    close();
    await sleep(3000);
    update("s-dark", { status: "idle", progress: null, subtitle: "Finished — ready for you" });
    sessions = sessions.filter((s) => s.id !== docs.id);
    await publish();
    await sleep(reducedMotion ? 1500 : 6000);
  }
}

ReactDOM.createRoot(document.getElementById("widget")!).render(
  <React.StrictMode>
    <Island />
  </React.StrictMode>,
);
setTimeout(play, 400);

// ---------- Theme picker: the screenshot and the live widget ----------

const SHOTS: Record<ThemeId, string> = {
  mono: shotMono,
  crew: shotCrew,
  terminal: shotTerminal,
  paper: shotPaper,
  pixel: shotPixel,
  glass: shotGlass,
};
const BLURBS: Record<ThemeId, string> = {
  mono: "Black and quiet, with one accent colour.",
  crew: "Deep navy, chunky type, and a little character for every session.",
  terminal: "Green phosphor on black, with an ASCII face for every session.",
  paper: "Light and warm: ink on paper, with a hand-drawn doodle per session.",
  pixel: "8-bit: a pixel-art sprite for every session, and a pixel font.",
  glass: "Translucent and glossy, with a coloured orb for every session.",
};
const radios = Array.from(document.querySelectorAll<HTMLButtonElement>(".theme-picker [data-theme]"));
const choose = (id: ThemeId, focus = false) => {
  const theme = THEMES.find((t) => t.id === id);
  if (!theme) return;
  config = { ...config, appearance: { ...config.appearance, theme: id } };
  emit("agentry://config", config);
  radios.forEach((b) => {
    const on = b.dataset.theme === id;
    b.setAttribute("aria-checked", String(on));
    b.tabIndex = on ? 0 : -1;
    if (on && focus) b.focus();
  });
  const img = document.getElementById("theme-img") as HTMLImageElement;
  img.src = SHOTS[id];
  img.alt = `The ${theme.name} theme`;
  document.getElementById("theme-blurb")!.textContent = BLURBS[id];
};
radios.forEach((b, i) => {
  b.tabIndex = i === 0 ? 0 : -1;
  b.addEventListener("click", () => choose(b.dataset.theme as ThemeId));
  // Arrow keys move between options, as in any radio group.
  b.addEventListener("keydown", (e) => {
    const step = e.key === "ArrowRight" || e.key === "ArrowDown" ? 1 : e.key === "ArrowLeft" || e.key === "ArrowUp" ? -1 : 0;
    if (!step) return;
    e.preventDefault();
    choose(radios[(i + step + radios.length) % radios.length].dataset.theme as ThemeId, true);
  });
});
// Warm the other screenshots so switching is instant.
Object.values(SHOTS).forEach((src) => (new Image().src = src));

// ---------- Copy buttons and the menu bar clock ----------

document.querySelectorAll<HTMLElement>("[data-copy]").forEach((box) => {
  const button = box.querySelector("button")!;
  button.addEventListener("click", async () => {
    try {
      await navigator.clipboard.writeText(box.querySelector("code")!.textContent ?? "");
      button.textContent = "Copied";
    } catch {
      button.textContent = "Select and copy";
    }
    setTimeout(() => (button.textContent = "Copy"), 1800);
  });
});

const clock = document.getElementById("clock")!;
const tick = () =>
  (clock.textContent = new Date().toLocaleString([], { weekday: "short", hour: "2-digit", minute: "2-digit" }));
tick();
setInterval(tick, 15_000);
