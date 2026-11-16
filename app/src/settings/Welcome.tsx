// First-run welcome: connect to Claude Code (with consent), optionally add
// the command-line tool, pick a look, and choose whether to open at login.

import "./settings.css";
import { accentStyle, themeFor } from "../themes";
import type { Config } from "../types";
import { finishWelcome, useConfig } from "../useSessions";
import { CliInstall, Integration, LaunchAtLogin, Section, ThemePicker } from "./Settings";
import { prettyShortcut } from "./Settings";

export function Welcome() {
  const { config, save } = useConfig();
  if (!config) return <div className="settings-page tokens-mono">Loading…</div>;
  const theme = themeFor(config);
  const update = (change: (c: Config) => void) => {
    const next: Config = JSON.parse(JSON.stringify(config));
    change(next);
    save(next).catch(() => {});
  };

  return (
    <div className={`settings-page welcome tokens-${theme.id}`} style={accentStyle(theme, config)}>
      <h1>Welcome to Agentry</h1>
      <p className="welcome-lead">
        Agentry sits under your menu bar and shows every Claude Code session: what it’s doing, how far along it is,
        and when it needs you. You can approve, deny and answer Claude from it without going back to the terminal.
      </p>

      <Section title="1 · Connect to Claude Code">
        <p className="hint">
          Agentry adds a few hooks to <code>~/.claude/settings.json</code> so Claude Code can tell it what’s happening.
          Your file is backed up first, nothing else in it changes, and you can remove them any time in Settings. When
          Agentry isn’t running, Claude Code works exactly as before.
        </p>
        <Integration />
      </Section>

      <Section title="2 · Pick a look">
        <ThemePicker config={config} update={update} />
      </Section>

      <Section title="3 · A few options">
        <LaunchAtLogin />
        <CliInstall />
        <p className="hint">
          Open the panel any time with {prettyShortcut(config.general.shortcut ?? "CommandOrControl+Alt+A")}, or from
          the menu bar icon. The first time you jump to a terminal tab, macOS asks whether Agentry may control that
          app; allow it so ↗ can find the right tab.
        </p>
      </Section>

      <div className="welcome-done">
        <button className="btn btn-primary" onClick={() => finishWelcome().catch(() => {})}>
          Done
        </button>
      </div>
    </div>
  );
}
