#!/usr/bin/env node
// Regenerate the screenshots in docs/images from the demo page (the real UI
// with made-up sessions; see app/src/demo). Needs Google Chrome and the UI's
// dependencies (`pnpm install` in app/). No other packages: it talks to
// Chrome over the DevTools protocol with Node's built-in WebSocket (Node 22+).
//
//   node scripts/screenshots.mjs            # all shots
//   node scripts/screenshots.mjs panel-mono # only shots whose name matches

import { spawn } from "node:child_process";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const out = join(root, "docs", "images");
const PORT = 1431;
const DEBUG_PORT = 9333;
const CHROME =
  process.env.CHROME ?? "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome";

/** name → demo page query. `width` is the browser window's width;
 *  `maxHeight` crops a tall page. */
const SHOTS = [
  { name: "panel-mono", q: "theme=mono&scene=panel" },
  { name: "pill-mono", q: "theme=mono&scene=pill" },
  { name: "question-mono", q: "theme=mono&scene=question" },
  { name: "transcript-mono", q: "theme=mono&scene=transcript&open=0" },
  { name: "warning-mono", q: "theme=mono&scene=warning" },
  { name: "theme-crew", q: "theme=crew&scene=panel" },
  { name: "theme-terminal", q: "theme=terminal&scene=panel" },
  { name: "theme-paper", q: "theme=paper&scene=panel" },
  { name: "theme-pixel", q: "theme=pixel&scene=panel" },
  { name: "theme-glass", q: "theme=glass&scene=panel" },
  { name: "settings", q: "view=settings&theme=mono", width: 900, selector: "#stage", maxHeight: 1050 },
];

const filter = process.argv[2];
const shots = SHOTS.filter((s) => !filter || s.name.includes(filter));
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

async function waitFor(url, tries = 100) {
  for (let i = 0; i < tries; i++) {
    try {
      const r = await fetch(url);
      if (r.ok) return r;
    } catch {}
    await sleep(100);
  }
  throw new Error(`timed out waiting for ${url}`);
}

/** A minimal DevTools protocol client for one page. */
async function connect(wsUrl) {
  const ws = new WebSocket(wsUrl);
  await new Promise((ok, fail) => {
    ws.onopen = ok;
    ws.onerror = fail;
  });
  let id = 0;
  const waiting = new Map();
  ws.onmessage = (m) => {
    const msg = JSON.parse(m.data);
    const w = waiting.get(msg.id);
    if (!w) return;
    waiting.delete(msg.id);
    msg.error ? w.fail(new Error(msg.error.message)) : w.ok(msg.result);
  };
  const send = (method, params = {}) =>
    new Promise((ok, fail) => {
      waiting.set(++id, { ok, fail });
      ws.send(JSON.stringify({ id, method, params }));
    });
  return { send, close: () => ws.close() };
}

const vite = spawn(
  join(root, "app", "node_modules", ".bin", "vite"),
  ["--port", String(PORT), "--strictPort"],
  { cwd: join(root, "app"), stdio: "ignore" },
);
const profile = mkdtempSync(join(tmpdir(), "agentry-shots-"));
const chrome = spawn(
  CHROME,
  [
    "--headless=new",
    `--remote-debugging-port=${DEBUG_PORT}`,
    `--user-data-dir=${profile}`,
    "--hide-scrollbars",
    "--no-first-run",
    "--force-color-profile=srgb",
    "about:blank",
  ],
  { stdio: "ignore" },
);

try {
  await waitFor(`http://localhost:${PORT}/demo.html`);
  await waitFor(`http://127.0.0.1:${DEBUG_PORT}/json/version`);
  const pages = await (await fetch(`http://127.0.0.1:${DEBUG_PORT}/json/list`)).json();
  const page = await connect(pages.find((p) => p.type === "page").webSocketDebuggerUrl);
  await page.send("Page.enable");
  await page.send("Runtime.enable");
  mkdirSync(out, { recursive: true });

  const evaluate = async (expression) =>
    (await page.send("Runtime.evaluate", { expression, returnByValue: true })).result.value;

  for (const shot of shots) {
    await page.send("Emulation.setDeviceMetricsOverride", {
      width: shot.width ?? 640,
      height: 1400,
      deviceScaleFactor: 2,
      mobile: false,
    });
    await page.send("Page.navigate", { url: `http://localhost:${PORT}/demo.html?${shot.q}` });
    for (let i = 0; i < 100 && !(await evaluate("document.body?.dataset.ready === '1'")); i++) {
      await sleep(100);
    }
    await evaluate("document.fonts.ready.then(() => true)");
    await sleep(400); // let the open animation settle
    // The island plus a margin of backdrop, or the whole selector's box.
    const box = await evaluate(`(() => {
      const sel = ${JSON.stringify(shot.selector ?? ".island-shape")};
      const r = document.querySelector(sel).getBoundingClientRect();
      const m = sel === ".island-shape" ? 32 : 0;
      return { x: Math.max(0, r.left - m), y: Math.max(0, r.top - m),
               width: r.width + 2 * m, height: r.height + 2 * m };
    })()`);
    if (shot.maxHeight) box.height = Math.min(box.height, shot.maxHeight);
    const { data } = await page.send("Page.captureScreenshot", {
      format: "png",
      clip: { ...box, scale: 1 },
      captureBeyondViewport: true,
    });
    writeFileSync(join(out, `${shot.name}.png`), Buffer.from(data, "base64"));
    console.log(`docs/images/${shot.name}.png  ${Math.round(box.width)}×${Math.round(box.height)}`);
  }
  page.close();
} finally {
  chrome.kill();
  vite.kill();
  await sleep(300);
  rmSync(profile, { recursive: true, force: true });
}
