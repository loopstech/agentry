// The Settings window. Every change is saved straight away and reaches the
// island instantly.

import "./settings.css";
import { useEffect, useId, useState, type ReactNode } from "react";
import { THEMES, accentStyle, themeFor } from "../themes";
import type { Theme } from "../themes/types";
import type { Config, Session } from "../types";
import {
  hooksStatus,
  installHooks,
  playSound,
  uninstallHooks,
  useConfig,
  type HooksStatus,
} from "../useSessions";
import pkg from "../../package.json";

const TIMEOUTS: [number | null, string][] = [
  [30, "30 seconds"],
  [60, "1 minute"],
  [90, "90 seconds"],
  [120, "2 minutes"],
  [300, "5 minutes"],
  [600, "10 minutes"],
  [1800, "30 minutes"],
  [null, "Never"],
];

const TOOLS: [string, string][] = [
  ["Bash", "Shell commands"],
  ["Write", "Creating files"],
  ["Edit", "Editing files"],
  ["MultiEdit", "Multiple edits"],
  ["NotebookEdit", "Notebooks"],
  ["WebFetch", "Opening web pages"],
  ["WebSearch", "Web searches"],
  ["Read", "Reading files outside the project"],
  ["mcp", "MCP tools"],
];

const MODES: [Config["approvals"]["mode"], string, string][] = [
  ["widget_first", "Agentry first", "Prompts appear in the widget; the terminal can still answer, and takes over after the timeout."],
  ["widget_when_visible", "Only while the panel is open", "When the panel is closed, prompts go straight to the terminal."],
  ["terminal_only", "Terminal only", "Agentry just shows which sessions need you."],
];

export function Settings() {
  const { config, save } = useConfig();
  const [error, setError] = useState<string | null>(null);
  if (!config) return <div className="settings-page tokens-mono">Loading…</div>;

  const theme = themeFor(config);
  const update = (change: (c: Config) => void) => {
    const next: Config = JSON.parse(JSON.stringify(config));
    change(next);
    save(next).then(
      () => setError(null),
      (e) => setError(String(e)),
    );
  };

  return (
    <div className={`settings-page tokens-${theme.id}`} style={accentStyle(theme, config)}>
      <h1>Settings</h1>
      {error && (
        <p className="settings-error" role="alert">
          Couldn’t save: {error}
        </p>
      )}

      <Section title="Appearance">
        <fieldset className="theme-picker">
          <legend>Theme</legend>
          {THEMES.map((t) => (
            <label key={t.id} className={`theme-card${t.id === theme.id ? " is-selected" : ""}`}>
              <input
                type="radio"
                name="theme"
                checked={t.id === theme.id}
                onChange={() =>
                  update((c) => {
                    c.appearance.theme = t.id;
                    // An accent picked for one theme rarely suits the other.
                    c.appearance.accent = null;
                  })
                }
              />
              <ThemePreview theme={t} config={config} />
              <span className="theme-card-name">{t.name}</span>
              <span className="theme-card-description">{t.description}</span>
            </label>
          ))}
        </fieldset>

        <fieldset className="field-group">
          <legend>Accent colour</legend>
          <div className="swatches">
            {theme.accents.map((a) => {
              const selected = (config.appearance.accent ?? theme.defaultAccent) === a;
              return (
                <label key={a} className={`swatch${selected ? " is-selected" : ""}`} style={{ background: a }}>
                  <input
                    type="radio"
                    name="accent"
                    checked={selected}
                    aria-label={a === theme.defaultAccent ? `${a} (theme default)` : a}
                    onChange={() => update((c) => (c.appearance.accent = a === theme.defaultAccent ? null : a))}
                  />
                </label>
              );
            })}
            <label className="swatch-custom">
              <span>Custom</span>
              <input
                type="color"
                value={config.appearance.accent ?? theme.defaultAccent}
                onChange={(e) => update((c) => (c.appearance.accent = e.target.value))}
              />
            </label>
          </div>
        </fieldset>

        <Toggle
          label="Open the panel when I hover over it"
          checked={config.appearance.hoverToOpen}
          onChange={(v) => update((c) => (c.appearance.hoverToOpen = v))}
        />
      </Section>

      <Section title="Approvals and questions">
        <fieldset className="field-group">
          <legend>Who answers permission prompts</legend>
          {MODES.map(([mode, label, hint]) => (
            <label key={mode} className="radio-row">
              <input
                type="radio"
                name="mode"
                checked={config.approvals.mode === mode}
                onChange={() => update((c) => (c.approvals.mode = mode))}
              />
              <span>
                <span className="radio-label">{label}</span>
                <span className="hint">{hint}</span>
              </span>
            </label>
          ))}
        </fieldset>

        <label className="select-row">
          <span>Hand prompts back to the terminal after</span>
          <select
            value={config.approvals.timeoutSecs ?? "never"}
            onChange={(e) =>
              update((c) => (c.approvals.timeoutSecs = e.target.value === "never" ? null : Number(e.target.value)))
            }
          >
            {TIMEOUTS.map(([v, label]) => (
              <option key={label} value={v ?? "never"}>
                {label}
              </option>
            ))}
            {config.approvals.timeoutSecs !== null && !TIMEOUTS.some(([v]) => v === config.approvals.timeoutSecs) && (
              <option value={config.approvals.timeoutSecs}>{config.approvals.timeoutSecs} seconds</option>
            )}
          </select>
        </label>

        <fieldset className="field-group">
          <legend>Prompts shown in Agentry</legend>
          <Toggle
            label="All tools"
            checked={config.approvals.tools === null}
            onChange={(all) => update((c) => (c.approvals.tools = all ? null : TOOLS.map(([t]) => t)))}
          />
          {config.approvals.tools !== null && (
            <div className="tool-grid">
              {TOOLS.map(([tool, label]) => (
                <label key={tool} className="check-row">
                  <input
                    type="checkbox"
                    checked={config.approvals.tools!.some((t) => t.toLowerCase() === tool.toLowerCase())}
                    onChange={(e) =>
                      update((c) => {
                        const rest = (c.approvals.tools ?? []).filter((t) => t.toLowerCase() !== tool.toLowerCase());
                        c.approvals.tools = e.target.checked ? [...rest, tool] : rest;
                      })
                    }
                  />
                  <span>{label}</span>
                </label>
              ))}
            </div>
          )}
        </fieldset>

        <Toggle
          label="Answer Claude’s questions in Agentry"
          checked={config.approvals.answerQuestions}
          onChange={(v) => update((c) => (c.approvals.answerQuestions = v))}
        />
        <Toggle
          label="Offer “Always allow” when Claude Code suggests a rule"
          checked={config.approvals.alwaysAllow}
          onChange={(v) => update((c) => (c.approvals.alwaysAllow = v))}
        />
        <Toggle
          label="Open the panel when something needs me"
          checked={config.approvals.autoOpen}
          onChange={(v) => update((c) => (c.approvals.autoOpen = v))}
        />
      </Section>

      <Section title="Sounds and quiet mode">
        <Toggle
          label="Quiet mode"
          hint="No sounds, and every prompt goes straight to the terminal."
          checked={config.muted}
          onChange={(v) => update((c) => (c.muted = v))}
        />
        <Toggle
          label="Play sounds"
          hint="When a session needs you, and when one finishes."
          checked={config.sounds.enabled}
          onChange={(v) => update((c) => (c.sounds.enabled = v))}
        />
        <label className="select-row">
          <span>Volume</span>
          <input
            type="range"
            min={0}
            max={100}
            step={5}
            value={Math.round(config.sounds.volume * 100)}
            onChange={(e) => update((c) => (c.sounds.volume = Number(e.target.value) / 100))}
          />
        </label>
        <div className="actions">
          <button className="btn" onClick={() => playSound("needs_you")}>
            Play “needs you”
          </button>
          <button className="btn" onClick={() => playSound("finished")}>
            Play “finished”
          </button>
        </div>
      </Section>

      <Section title="Claude Code">
        <Integration />
      </Section>

      <Section title="About">
        <p className="hint">
          Agentry {pkg.version} · MIT licence · github.com/loopstech/Agentry
        </p>
      </Section>
    </div>
  );
}

function Section({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section className="settings-section">
      <h2>{title}</h2>
      {children}
    </section>
  );
}

function Toggle(props: { label: string; hint?: string; checked: boolean; onChange: (v: boolean) => void }) {
  const id = useId();
  return (
    <div className="toggle-row">
      <label htmlFor={id}>
        <span className="toggle-label">{props.label}</span>
        {props.hint && <span className="hint">{props.hint}</span>}
      </label>
      <input
        id={id}
        type="checkbox"
        role="switch"
        className="switch"
        checked={props.checked}
        onChange={(e) => props.onChange(e.target.checked)}
      />
    </div>
  );
}

function Integration() {
  const [status, setStatus] = useState<HooksStatus | null>(null);
  const [message, setMessage] = useState<{ text: string; error: boolean } | null>(null);
  const refresh = () => hooksStatus().then(setStatus, (e) => setMessage({ text: String(e), error: true }));
  useEffect(() => {
    refresh();
  }, []);
  const run = (action: () => Promise<string>) =>
    action().then(
      (text) => {
        setMessage({ text, error: false });
        refresh();
      },
      (e) => setMessage({ text: String(e), error: true }),
    );

  if (!status) return <p className="hint">Checking…</p>;
  const complete = status.installed >= status.expected && status.missingPrograms.length === 0;
  return (
    <div className="integration">
      <p>
        {complete
          ? "Agentry is connected to Claude Code."
          : status.installed === 0
            ? "Agentry isn’t connected to Claude Code yet."
            : "Agentry’s connection to Claude Code needs repairing."}
      </p>
      <p className="hint">
        Hooks in {status.settingsFile}: {status.installed} of {status.expected}
        {status.claudeVersion && <> · {status.claudeVersion}</>}
      </p>
      {status.missingPrograms.length > 0 && (
        <p className="hint">Points at a program that no longer exists: {status.missingPrograms.join(", ")}</p>
      )}
      <div className="actions">
        <button className="btn btn-primary" disabled={!status.hookProgram} onClick={() => run(installHooks)}>
          {status.installed ? "Reinstall hooks" : "Connect to Claude Code"}
        </button>
        {status.installed > 0 && (
          <button className="btn" onClick={() => run(uninstallHooks)}>
            Remove hooks
          </button>
        )}
      </div>
      {!status.hookProgram && (
        <p className="hint">The agentry-hook program wasn’t found next to the app, so hooks can’t be installed from here.</p>
      )}
      {message && <p className={message.error ? "settings-error" : "hint"}>{message.text}</p>}
      <p className="hint">Your settings file is backed up before every change, and only Agentry’s own entries are touched.</p>
    </div>
  );
}

/** A small live sample of a theme: the pill with three sessions. */
function ThemePreview({ theme, config }: { theme: Theme; config: Config }) {
  const style = theme.id === themeFor(config).id ? accentStyle(theme, config) : accentStyle(theme, null);
  const sample = SAMPLE.map((s) => ({ ...s }));
  return (
    <span className={`theme-preview ${theme.className}`} style={style} aria-hidden="true">
      <span className="pill pill-needs">
        <span className="pill-avatars">
          {sample.map((s, i) => (
            <span key={s.id} className="pill-avatar" style={{ zIndex: 3 - i }}>
              <theme.Avatar session={s} size="sm" />
            </span>
          ))}
        </span>
        <span className="pill-strong">{theme.strings.pillSummary({ needs: 1, working: 2, ready: 0, done: 0 }).strong}</span>
      </span>
    </span>
  );
}

const base: Session = {
  id: "",
  cwd: "",
  project: "",
  title: "",
  titleSource: "prompt",
  status: "working",
  subtitle: "",
  progress: null,
  tasks: [],
  helpers: [],
  terminal: { ppid: null, tty: null, env: {} },
  transcriptPath: null,
  model: null,
  permissionMode: null,
  avatar: 0,
  startedAt: 0,
  updatedAt: 0,
  endedAt: null,
  needsSince: null,
  pinned: false,
  pending: [],
  backfilled: false,
};

const SAMPLE: Session[] = [
  { ...base, id: "a", project: "web", status: "needs_permission", avatar: 1 },
  { ...base, id: "b", project: "api-server", status: "working", avatar: 0 },
  { ...base, id: "c", project: "docs", status: "working", avatar: 2 },
];
