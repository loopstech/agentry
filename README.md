# Agentry

A small always-on-top macOS widget that shows every running Claude Code session. You can approve permissions and answer Claude's questions from it, and jump back to the session's window.

> **Status: Phase 6 (ready to release).** Every session shows live in the island. You can approve, deny and answer Claude from it, open a session's recent steps, and jump to its window. It has six themes and a Settings window, lives in the menu bar, and ships as a signed app with auto-updates (once the release secrets are set up; see [RELEASING.md](RELEASING.md)).

## Install

**Install script** (downloads the latest release, checks its checksum, installs the app and the `agentry` command, and asks before connecting to Claude Code):

```sh
curl -fsSL https://raw.githubusercontent.com/loopstech/agentry/main/scripts/install.sh | sh
```

**Homebrew:**

```sh
brew install --cask loopstech/tap/agentry
```

**Download:** grab `Agentry.dmg` from [Releases](https://github.com/loopstech/agentry/releases) and drag Agentry to Applications.

On first launch a welcome screen offers to **Connect to Claude Code**: it adds Agentry's hooks to `~/.claude/settings.json`, backing the file up first. You can also install the `agentry` command and choose a look there.

- **Open the panel:** click the pill under the menu bar (click the panel's header to close it again), press **⌥⌘A** from any app, or use the menu bar icon.
- **Chat with a session:** the speech-bubble button on a row shows Claude's latest reply as a bubble and adds a box for messaging Claude. Sending works for sessions in tmux, iTerm2 or WezTerm.
- **Approve from the keyboard:** with the panel open, **⌘↵** allows and **⌘⌫** denies the top request.

### Uninstall

- **Homebrew:** `brew uninstall --cask agentry` removes the hooks too. Add `--zap` to also remove settings.
- **Otherwise:**

  ```sh
  agentry hooks uninstall     # removes only Agentry's hooks; your settings are backed up first
  rm -rf /Applications/Agentry.app ~/.agentry
  rm -f "$(command -v agentry)"
  ```

  Then turn off "Open Agentry when I log in" in Settings before deleting the app, or remove `~/Library/LaunchAgents/Agentry.plist`.

## How it works

Claude Code runs `agentry-hook` for each hook event. The hook forwards the event to the Agentry app over `127.0.0.1`, using a token stored in `~/.agentry/token`.

If the app isn't running, the hook exits immediately and silently, so Claude Code behaves exactly as if Agentry weren't installed.

| Piece | What it is |
|---|---|
| `agentry-hook` | A tiny Rust program (~0.5 MB). Claude Code runs it for each event. |
| `agentry` | The command-line tool: install and remove hooks, `status`, `doctor`. |
| Agentry app | Tauri app. Runs the local server and the island widget. |
| `agentry-devserver` | The app's server without a window, for development. |

## Testing from source on a Mac

You need the Xcode Command Line Tools, Rust (via [rustup](https://rustup.rs)), Node 20+ and pnpm (`npm i -g pnpm`).

```sh
git clone https://github.com/loopstech/agentry && cd agentry
git checkout claude/upbeat-sagan-igll5h

# 1. Build the hook and the CLI
cargo build --release -p agentry-hook -p agentry-cli

# 2. Start the app (first build takes a few minutes)
cd app && pnpm install && pnpm tauri dev
```

In a second terminal:

```sh
cd agentry
./target/release/agentry hooks install   # backs up ~/.claude/settings.json first
./target/release/agentry doctor          # everything should be ✓
```

A small black pill appears just under the menu bar. Click it to open the panel. Drag it to move it; it reopens where you left it. If you drop it near a screen edge or the bottom, it moves back just enough for the open panel to fit.

### Notch mode checks (MacBooks with a notch)

1. **Attached.** The island grows out of the notch instead of floating below the menu bar:
   - collapsed, it's a black shape fused to the notch, with session avatars on the left and the most important count on the right (orange = needs you, ringed = working, grey = ready);
   - with no sessions it's exactly the notch's size and disappears into it. Clicking the notch still opens the panel.
2. **Expanded.** Click it, or press ⌥⌘A. The panel drops down from the notch, with a black band under the notch so it blends in (Crew's navy starts below it).
3. **Setting.** Settings → Appearance → *Attach to the notch* (on by default). Turn it off and the island goes back to floating below the menu bar, draggable, where you last left it.
4. **Other apps in front.** Click into VS Code (or any app), make an app full screen, switch desktops, and turn Stage Manager on if you use it. The island stays at the notch the whole time.
5. **External screens.** Drag the island to a screen without a notch (with the setting off), turn the setting back on, and it stays on that screen, floating. Notch mode only applies on the screen that has the notch.

Tell me what you see: does the shape line up exactly with your notch (width, height, corners)? Does it cover menu bar items you use next to the notch?

### Phase 6 checks

1. **Menu bar.** There's no Dock icon. A menu bar icon offers Show or Hide, Settings…, Quiet Mode and Quit. "Show a Dock icon" in Settings brings the Dock icon back.
2. **Shortcut.** Press **⌥⌘A** in any app: the panel opens, and pressing it again closes it. With a request waiting, ⌘↵ allows it and ⌘⌫ denies it. Change or turn off the shortcut in Settings.
3. **Full-screen apps.** Put a terminal in full screen. The island should still float on top.
4. **Only one Agentry.** Launching it again just opens the panel of the running one.
5. **Welcome.** Remove `"onboarded"` from `~/.agentry/config.json` and relaunch. The welcome screen appears and walks through connecting, installing the command and picking a theme.
6. **Open at login.** Toggle it in Settings (General), log out and back in.
7. **Release build.** Follow [RELEASING.md](RELEASING.md) → *Building locally*. The app should contain `Contents/MacOS/agentry-hook` and `agentry`, and "Connect to Claude Code" should point at those.

### Phase 5 checks

1. **Settings.** Click the gear in the panel header. The Settings window opens.
2. **Theme.** Click the **Crew** card. The island switches straight away:
   - characters instead of initials, with wide eyes and a "!" or "?" badge when waiting;
   - step dots, a "Helpers" box, and "Go for it" / "Nope…" buttons.
   - Quit and reopen Agentry: it stays on Crew.
   - Try the others too: **Terminal** (green phosphor, ASCII faces, a blinking cursor while working), **Paper** (the light theme, hand-drawn doodles), **Pixel** (8-bit sprites that hop and bob) and **Glass** (translucent, glossy orbs).
3. **Accent.** Pick a swatch, or **Custom**. The island's accent changes instantly.
4. **Sounds.** Use **Play "needs you"** and **Play "finished"**, then the volume slider. Then confirm sounds also play on their own:
   - when a session starts waiting on you (Mono: Tink, Crew: Pop, Terminal: Morse, Paper: Ping, Pixel: Funk, Glass: Glass);
   - when one finishes (Mono: Purr, Crew: Hero, Terminal: Bottle, Paper: Blow, Pixel: Frog, Glass: Submarine).
   - Quiet mode silences them.
5. **Hover to open.** Turn it on; resting the pointer on the pill opens the panel, and moving away closes it again.
6. **Connect to Claude Code.** The Claude Code section shows whether hooks are installed, and can install or remove them. It backs up your settings first.
7. **Terminal.** `agentry config set appearance.theme mono` also switches the island live.

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
agentry config [get KEY | set KEY VALUE]   # same settings as the Settings window
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
