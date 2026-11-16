# Agentry

A small always-on-top macOS widget that shows every running Claude Code session. You can approve permissions and answer Claude's questions from it, and jump back to the session's window.

> **Status: Phase 4 (detail view and jump).**
> - Every session shows live in the island.
> - You can allow or deny permission prompts and answer Claude's questions from it.
> - Clicking a session shows its recent steps; ↗ jumps to its terminal tab or editor window.
> - Sessions that were already running when the app started are picked up.
>
> Next: the Crew theme and a Settings window. See [PLAN.md](PLAN.md).

## How it works

Claude Code runs `agentry-hook` for each hook event. The hook forwards the event to the Agentry app over `127.0.0.1`, using a token stored in `~/.agentry/token`.

If the app isn't running, the hook exits immediately and silently, so Claude Code behaves exactly as if Agentry weren't installed.

| Piece | What it is |
|---|---|
| `agentry-hook` | A tiny Rust program (~0.5 MB). Claude Code runs it for each event. |
| `agentry` | The command-line tool: install and remove hooks, `status`, `doctor`. |
| Agentry app | Tauri app. Runs the local server and the island widget. |
| `agentry-devserver` | The app's server without a window, for development. |

## Testing on a Mac

You need the Xcode Command Line Tools, Rust (via [rustup](https://rustup.rs)), Node 20+ and pnpm (`npm i -g pnpm`).

```sh
git clone https://github.com/loopstech/Agentry && cd Agentry
git checkout claude/upbeat-sagan-igll5h

# 1. Build the hook and the CLI
cargo build --release -p agentry-hook -p agentry-cli

# 2. Start the app (first build takes a few minutes)
cd app && pnpm install && pnpm tauri dev
```

In a second terminal:

```sh
cd Agentry
./target/release/agentry hooks install   # backs up ~/.claude/settings.json first
./target/release/agentry doctor          # everything should be ✓
```

A small black pill appears just under the menu bar. Click it to open the panel. Drag it to move it; it reopens where you left it.

### Phase 4 checks

1. **Detail view.** Click a session's title. Its recent steps appear (your prompts, Claude's replies, and tool calls ticked when done or crossed when they fail), and they keep updating while it works.
2. **Jump.** Switch to another window, then click ↗ on a session.
   - **iTerm2 or Terminal.app:** the exact tab comes forward. The first time, macOS asks whether Agentry may control that app; allow it.
   - **VS Code or Cursor:** the project's window comes forward.
   - **Inside tmux:** the right pane is selected.
3. **Use terminal.** On an approval card, *Use terminal* hands the prompt back and jumps to the session.
4. **Already running.** Start a `claude` session, then quit and restart Agentry.
   - The session reappears straight away, titled with Claude Code's own title, as "Active before Agentry started".
   - Its ↗ works once it does something.
5. **Titles.** `/rename Something` in a session updates its card within a few seconds.

Please tell me which terminal and editor you use, and whether ↗ found the right tab. Those AppleScript paths can't be tested here.

### Phase 3 checks

1. **Allow.** Ask Claude to run a command you haven't allowed, e.g. *"run: date > stamp.txt"*.
   - The panel opens with the command and **Allow / Deny… / Use terminal**.
   - Claude Code shows its own prompt in the terminal at the same time; either one can answer.
   - Click **Allow**: the terminal prompt closes and the command runs.
2. **Deny with a note.** Ask for something like *"run: rm stamp.txt"*.
   - Click **Deny…**, type *"keep it, I need it"*, and press Enter.
   - Claude should acknowledge your note and not delete the file.
3. **Always allow.** When the card offers it (e.g. "Always allow Bash(npm test:*) in this project"), click it.
   - The rule is saved to the project's `.claude/settings.local.json`.
   - Claude doesn't ask about that command again.
4. **Questions.** Ask *"use AskUserQuestion to ask me which fruit I want: Apple, Banana or Cherry"*.
   - Pick one in the widget and click **Send**.
   - The terminal shows "User answered Claude's questions: … → Cherry" and Claude carries on.
5. **Timeout and quiet mode.**
   - If you ignore a card, it hands over to the terminal after 90 s.
   - The speaker button in the panel header turns on quiet mode, which sends every prompt straight to the terminal.

Settings (until the Settings window in Phase 5):

```sh
./target/release/agentry config                          # list everything
./target/release/agentry config set approvals.timeout 120
./target/release/agentry config set approvals.mode widget_when_visible
./target/release/agentry config set approvals.tools Bash,Write,Edit
```

### Phase 2 checks

1. **Live progress.** In a `claude` session, ask: *"make a 5-step todo list and work through it slowly"*.
   - The card shows `n/5` with a thin progress bar.
   - The subtitle follows the current step.
   - When Claude finishes, the card reads "Finished — ready for you".
2. **Nested subagents.** Ask for two Explore subagents, e.g. *"use two Explore agents in parallel to look at different folders"*.
   - Both appear under the session's card.
   - They fade out about 30 s after they finish.
3. **Needs you.** Trigger a permission prompt, e.g. ask Claude to run a command you haven't allowed.
   - The card turns orange and jumps to the top, showing what Claude wants to run and how long it's been waiting.
   - The pill says "1 needs you" and pulses.
   - Answer in the widget or the terminal, and the card goes back to working.
4. **Housekeeping.** ✕ hides a session until you prompt it again. `./target/release/agentry status` lists the same sessions in the terminal.

These need a real Mac, so please check and tell me what you see:
- Is the panel's background transparent around the rounded corners, with a soft shadow?
- Does the pill stay on top of other windows?
- Does it follow you across desktops (Spaces)?
- Does dragging work, and does the position survive a restart?

### Phase 1 checks (still valid)

- **Quitting the app changes nothing.** Quit with ⌘Q and keep using Claude Code; there's no delay, because `agentry-hook` exits within milliseconds when the app is down.
- **Uninstall leaves settings intact.** Run `./target/release/agentry hooks uninstall`. Your `~/.claude/settings.json` should match the backup made before install, including your own hooks:

  ```sh
  diff <(jq -S . ~/.claude/settings.json) <(jq -S . "$(ls -1 ~/.claude/settings.json.agentry-backup-* | head -1)")
  ```

## Commands

```
agentry hooks install [--project [DIR]] [--hook-path PATH]
agentry hooks uninstall [--project [DIR]]
agentry status      # lists live sessions
agentry config [get KEY | set KEY VALUE]
agentry doctor
agentry version
```

- **Scope.** `--project` uses `<DIR>/.claude/settings.local.json` instead of your user settings. Don't install in both places, or every event is sent twice. `doctor` warns if you have.
- **Backups.** Install and uninstall back the settings file up first, as `settings.json.agentry-backup-<ms>`, and keep the last 5.
- **What gets touched.** Only entries whose command runs `agentry-hook` are added or removed. Everything else in the file is kept, in its original order.
- **Turning it off.** Set `AGENTRY_DISABLE=1` in a session's environment to make the hook do nothing for that session.

## Development

```sh
cargo test --workspace                               # unit + end-to-end tests (needs app/dist: run `pnpm build` in app/ first)
cargo run -p agentry-server --bin agentry-devserver  # headless server; RUST_LOG=debug also logs payload field names
cd app && pnpm tauri dev                             # the app
```

- **Separate test locations.** Set `AGENTRY_HOME` and `CLAUDE_CONFIG_DIR` to test without touching your real `~/.agentry` and `~/.claude`.
- **Linux builds.** Building the app on Linux needs the WebKitGTK development packages (see `.github/workflows/ci.yml`).

## Privacy

- The server only listens on `127.0.0.1` and requires the token.
- Tool inputs and outputs are kept in memory only, never logged or written to disk.
- There is no telemetry.

## License

[MIT](LICENSE)
