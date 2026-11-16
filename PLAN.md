# Agentry — Build Plan

Agentry is a small, always-on-top macOS widget that shows every running Claude Code session. For each one it shows the title, current step, and task progress. It shows subagents nested under their parent session. You can approve permissions and answer Claude's questions inline. You can jump back to the session's window, or dismiss the session when it's done.

It's a public, open-source (MIT) app. It installs from the command line, Homebrew, or a direct download.

> **Status:** planning. No code until the owner says go. Build phase by phase and stop after each phase for testing.

---

## 1. Decisions so far

| Topic | Decision |
|---|---|
| App framework | **Tauri 2**: a Rust core with a React + TypeScript UI. It downloads at ~10 MB and uses ~40 MB of RAM, versus Electron's ~150 MB download and ~200 MB+ of RAM. |
| Hook program | **`agentry-hook`**, a compiled Rust binary bundled in the `.app`. It needs no Node and starts in under 5 ms. |
| Platform | macOS first (Apple Silicon and Intel). OS-specific code stays behind traits so Linux and Windows can be added later. |
| Where sessions run | All the common places: Terminal.app, iTerm2, Ghostty, Warp, kitty and WezTerm, plus the VS Code, Cursor and Windsurf extensions. |
| Approvals | **Customisable.** By default the widget answers first and falls back to the terminal after a timeout you choose (§6). |
| Titles | Claude Code's own session title if there is one. Otherwise `<folder> · <first words of first prompt>`. No AI calls. |
| Subagents | **Nested** under the parent card, as a "Helpers" list. |
| Look | **Island** form factor: a pill under the menu bar that expands into a panel. Two user-selectable themes, **Mono** (direction D) and **Crew** (direction E), each with a customisable accent colour. |
| Distribution | Signed and notarised `.app`, published to **GitHub Releases**, a **Homebrew cask**, and a **one-line install script**. Includes **auto-update**. |
| CLI | An `agentry` command for installing and removing hooks, checking status, and diagnosing problems without opening the app. |
| Licence | MIT |

Design reference (private canvas): https://claude.ai/artifact/8dNMzdw8LY5crzjtgXnY31. The boards to build from are **D · Island** and **E · Crew Island**.

---

## 2. Claude Code integration (checked against the hooks docs, 2026-11-16)

Source: https://code.claude.com/docs/en/hooks. At the start of Phase 1, re-check these facts against the docs and against `claude --version`. Record any differences in `NOTES.md`.

### Events we register

| Event | Mode | What it's for |
|---|---|---|
| `SessionStart` | async | Create the session. Gives us `source` (startup, resume, clear, compact or fork), `session_title` and `model`. |
| `UserPromptSubmit` | async | Store the first prompt and set the status to working. |
| `PreToolUse`, matcher `AskUserQuestion` | **blocking** | Answer Claude's questions from the widget. |
| `PermissionRequest` | **blocking** | Allow or deny from the widget. Fires only when Claude Code would actually ask. |
| `PostToolUse` / `PostToolUseFailure` | async | Last activity, subtitle, and TodoWrite progress. |
| `TaskCreated` / `TaskCompleted` | async | Progress for the newer task tools. |
| `SubagentStart` / `SubagentStop` | async | The nested "Helpers" list. |
| `Notification` | async | `permission_prompt`, `idle_prompt` and `agent_needs_input`, which mean "needs you in the terminal". |
| `Stop` / `StopFailure` | async | Finished and ready for you, or errored. |
| `SessionEnd` | async | Mark the session done and schedule its removal. |

- **Async hooks** (`"async": true`) run in the background, so status updates add **zero latency** to Claude.
- **Blocking hooks** get a long `timeout` (3600 s). The app decides how long to actually wait (§6), so the hook timeout is only a safety net.
- **Subagent tool events** carry `agent_id` and `agent_type`. The store attaches them to the parent session's Helpers list instead of creating a new card.

### Permission decision (PermissionRequest)

```json
{ "hookSpecificOutput": { "hookEventName": "PermissionRequest",
    "decision": { "behavior": "allow" } } }
```

- **Deny:** `"behavior": "deny"` with `"message": "<note to Claude>"`. Add `"interrupt": true` if the user chose "Deny & stop".
- **Always allow:** `"behavior": "allow"` with `"updatedPermissions": [...]`, built from the request's `permission_suggestions`.
- **Hand back to the terminal:** exit 0 with no output, and Claude Code shows its normal prompt.
- A hook's "allow" never overrides a deny rule the user has configured. This is good, and nothing needs handling.

### Answering questions (PreToolUse on AskUserQuestion)

Return `allow` together with `updatedInput`. Echo back the original `questions` array and add `answers`, which maps each question's text to the chosen label. Multi-select labels are joined with commas.

```json
{ "hookSpecificOutput": { "hookEventName": "PreToolUse",
    "permissionDecision": "allow",
    "updatedInput": { "questions": [ ... original ... ],
                      "answers": { "Which framework?": "React" } } } }
```

If this behaves unexpectedly in testing, fall back to `deny` with the answer in the reason, and document it.

### Fail-open rules (non-negotiable)

- If the app isn't running, connecting must fail within **300 ms**. Exit 0 with no output, and Claude Code behaves as if Agentry didn't exist.
- Malformed responses, timeouts and crashes are handled the same way: exit 0 with no output.
- A hook must never hang a session because of the app.

---

## 3. Architecture

```
Claude Code session(s)
   │ hook JSON on stdin
   ▼
agentry-hook (Rust binary inside Agentry.app)
   │ HTTP over 127.0.0.1, with token header
   ▼
Agentry.app — Tauri core (Rust)
   • HTTP server (axum), bound to 127.0.0.1 only
   • Session store + event reducer
   • Pending-request broker (holds blocking hooks open)
   • Transcript tailer (JSONL)
   • Window focus (AppleScript, CLI integrations)
   • Updater, tray, launch at login
   │ Tauri events / commands
   ▼
UI — React + TypeScript + Vite
   • Island pill  ⇄  Expanded panel
   • Settings window
   • Themes: Mono, Crew
```

The `agentry` CLI is the same Rust codebase, built as a second binary. Install puts a symlink to it on the user's PATH.

### Repo layout

```
Agentry/
  Cargo.toml                    # workspace
  crates/
    agentry-core/               # shared types, config, settings.json merge logic
    agentry-hook/               # hook binary (tiny; std + minimal HTTP)
    agentry-cli/                # `agentry` command
  app/
    src-tauri/                  # Tauri app: server, store, broker, transcript, focus, updater
    src/                        # React UI
      themes/mono/  themes/crew/
      components/  state/
  scripts/
    install.sh                  # curl | sh installer
  packaging/
    homebrew/agentry.rb         # cask template
  docs/
  PLAN.md  NOTES.md  README.md  LICENSE
```

---

## 4. Hook program (`agentry-hook`)

1. Read stdin and parse the JSON.
2. Add context Claude Code doesn't send:
   - the parent process ID;
   - the TTY, from `ps -o tty= -p <ppid>`;
   - `TERM_PROGRAM`, `TERM_SESSION_ID`, `ITERM_SESSION_ID`, `KITTY_WINDOW_ID`, `WEZTERM_PANE`, `VSCODE_PID`, `__CFBundleIdentifier`.
3. Read the port and token from `~/.agentry/`.
4. Handle the event by type:
   - **Async events:** POST `/event` with a 300 ms connect timeout, then exit 0.
   - **`PermissionRequest` / `PreToolUse(AskUserQuestion)`:** POST `/request` and wait for the response.
     - On a decision, print the JSON above and exit 0.
     - On `{"decision":"terminal"}` or any error, print nothing and exit 0.
5. Never write tool input to disk or to logs.

---

## 5. Installing and removing hooks

The installer is used by three entry points: the CLI (`agentry hooks install`), the app's first-run screen (an explicit consent button), and `install.sh`.

- **Scope:**
  - user scope (`~/.claude/settings.json`) by default;
  - `--project` for `<repo>/.claude/settings.local.json`;
  - also detects managed (enterprise) settings, and warns if they block hooks.
- **Backup:** before every write, save `settings.json.agentry-backup-<timestamp>`, keeping the last 5.
- **Merge, never replace.** Each Agentry entry's command is the absolute path to `agentry-hook`, so uninstall finds Agentry's entries by that path and leaves the user's own hooks alone.
- **Version check:** read `claude --version`. If the version is older than the minimum that supports `PermissionRequest`, `async`, `TaskCreated` and so on, register only what's supported and say so.
- **Moving the app:** if the `.app` moves, `agentry doctor` (or the app at launch) detects stale hook paths and rewrites them.
- **Uninstall:** `agentry hooks uninstall` removes only Agentry's entries. Dragging the app to the Trash leaves the hooks in place, but they fail open. `brew uninstall` runs the uninstall step through a cask `uninstall_preflight`.

### CLI commands

```
agentry hooks install [--project]
agentry hooks uninstall [--project]
agentry status
agentry doctor
agentry open
agentry config get|set <key> <value>
agentry version
```

- **`status`** lists live sessions in the terminal.
- **`doctor`** checks the hooks, the app's reachability, the Claude Code version, stale paths and the token.

---

## 6. Approval policy (customisable)

Settings → Approvals:

| Setting | Options | Default |
|---|---|---|
| Who answers | **Widget first** · **Terminal only** (the widget just shows "needs you" and jumps) · **Widget only while visible** | Widget first |
| Fallback timeout | 15 s – 30 min, or never | 90 s |
| Tools shown inline | Any subset; the rest go to the terminal | All |
| "Always allow" button | On / off | On |
| Answering questions | On / off | On |
| Sound when something needs you | On / off, plus volume | On |

- **Countdown.** A waiting card shows a countdown ("back to terminal in 0:48"). When it reaches 0 the hook returns with no output, and the terminal shows its prompt.
- **The terminal is frozen meanwhile.** While the widget is waiting for an answer, the terminal can't show its own prompt. This trade-off is why the timeout is short by default, and the settings screen explains it.
- **Muted or closed widget.** If the user has muted Agentry, closed it, or (in "only while visible" mode) collapsed it, blocking requests go straight back to the terminal with no wait.

---

## 7. Server, store and data model

- **Server:** binds to `127.0.0.1` only, on a random free port saved to `~/.agentry/port`. A random token in `~/.agentry/token` (mode 0600) is required as an `X-Agentry-Token` header.

| Route | Purpose |
|---|---|
| `POST /event` | Apply the event to the store and push it to the UI |
| `POST /request` | Create a pending request and hold the connection until a decision, the timeout, or a policy hand-off |
| `GET /health` | Used by `agentry doctor` |

```ts
type Status = 'working' | 'needs_permission' | 'needs_answer' | 'needs_terminal' | 'idle' | 'error' | 'done';

interface Session {
  id: string; cwd: string; project: string;
  title: string; titleSource: 'claude' | 'fallback';
  status: Status; subtitle: string;
  progress?: { done: number; total: number; label: string };
  helpers: Helper[];                 // nested subagents
  pending?: PendingRequest;
  terminal: TerminalInfo;            // for jump-to-window
  avatar: number;                    // stable per session (theme picks the art)
  startedAt: number; updatedAt: number; pinned: boolean;
}
interface Helper { agentId: string; type: string; subtitle: string; status: 'working' | 'done' }
interface PendingRequest {
  id: string; kind: 'permission' | 'question';
  toolName: string; summary: string; detail?: string;     // e.g. the bash command
  questions?: { question: string; header?: string; options: { label: string; description?: string }[]; multiSelect?: boolean }[];
  suggestions?: unknown[];           // permission_suggestions, for "Always allow"
  expiresAt?: number;
}
```

### Reducer rules

- **Title:** use `session_title` from `SessionStart` when it's present, and pick up later changes from the transcript. Otherwise use `<folder> · <first 5 words of first prompt>`.
- **Progress:** from TodoWrite's `tool_input.todos`, or from `TaskCreated`/`TaskCompleted`.
  - The subtitle is the in-progress item's `activeForm`.
  - The label is "tasks".
- **Other tool use:** builds a human subtitle, for example "Reading server.ts", "Running tests" or "Editing 3 files".
- **Helpers:** events with an `agent_id` update or create a Helper on the parent session. `SubagentStop` marks it done, and finished helpers fade out after 30 s.
- **`Notification`:** `permission_prompt` with nothing pending sets the status to `needs_terminal`.
- **`Stop`:** sets the status to `idle`, with "Finished — ready for you". **`StopFailure`:** sets `error`.
- **`SessionEnd`:** sets `done` and auto-removes the session after 10 min unless it's pinned. Sessions with no events for 2 h are pruned.
- **Sort order:** sessions that need you go first, then working, idle and done.
- **Backfill on launch:** scan `~/.claude/projects/*/*.jsonl` modified in the last 30 min and show those sessions as "seen recently" until their next event arrives.

---

## 8. Jump to window

Each app has its own adapter behind a `Focus` trait, tried in order of precision:

| App | How | Precision |
|---|---|---|
| iTerm2 | AppleScript, matching `ITERM_SESSION_ID` | exact tab |
| Terminal.app | AppleScript, matching the TTY | exact tab |
| kitty | `kitty @ focus-window --match id:$KITTY_WINDOW_ID` (needs remote control enabled) | exact window |
| WezTerm | `wezterm cli activate-pane --pane-id $WEZTERM_PANE` | exact pane |
| Ghostty, Warp | `open -b <bundle id>` | app only |
| VS Code, Cursor, Windsurf | `<scheme>://file/<cwd>`, or the `code`/`cursor` CLI | project window |
| Unknown | Activate the parent app found by walking up the process tree | app only |

- **Permissions:** the first time AppleScript runs, macOS asks for Automation permission. The first-run screen explains this.

---

## 9. UI

### Form factor (both themes)

- **Collapsed pill.**
  - Position: top centre, just under the menu bar (it sits beside the notch on notched Macs). It can be dragged to another position, which is remembered.
  - Contents: overlapping avatars, then "N need you · N busy · N done", then an action.
  - It pulses gently when something new needs you.
- **Expanded panel.**
  - Opens by click, or by hover if enabled in settings.
  - About 520 px wide, rounded, and anchored to the pill.
  - Sessions are listed in sort order. Waiting cards expand inline with their controls.
- **Window behaviour:**
  - frameless, transparent, always on top, and visible on every desktop and in full-screen apps;
  - no Dock icon by default, with a menu bar icon instead.
- **Keyboard:**
  - a global shortcut (default ⌥⌘A) toggles the panel;
  - ⌘↵ allows the top request and ⌘⌫ denies it.
- **Detail view.** Clicking a card shows the last 50 transcript steps. The tailer runs only while the view is open.
- **Settings window** has these sections:
  - General: theme, accent, position, hover-to-open, launch at login, Dock icon.
  - Approvals: the §6 settings.
  - Sounds.
  - Integrations: hook status and the install/uninstall buttons, plus terminal and editor adapters.
  - Updates.
  - About.

### Themes

Each theme is a token set, plus an avatar set, plus a copy voice. The layout is the same for both.

| | **Mono** (D) | **Crew** (E) |
|---|---|---|
| Surfaces | Pure black panel, `#1F1F1F` dividers, row list | Navy `#1C1D4A` panel, `#272960` rounded cards |
| Type | Geist + Geist Mono | Bricolage Grotesque |
| Accent (default) | `#FF7A45` orange (user-changeable) | `#FFD166` sun (user-changeable) |
| Avatars | Initials in circles; the accent fill means it needs you | Characters (robot, blob, ghost, triangle, …) with "!" or "?" badges |
| Progress | Hairline bar | Step dots |
| Buttons | "Allow ⌘↵" · "Deny…" · "Open in terminal ↗" | "Go for it" · "Nope…" · terminal icon |
| Subagents | Indented list with a rule | "Helpers" box with mini characters |
| Motion | Minimal fades and height transitions | Bouncy expand, avatars bob while working, eyes widen when waiting |
| Sounds | Soft clicks | Playful chimes |

- **Theme switching:**
  - Themes live under `app/src/themes/<name>/` as tokens, avatar components, strings and sounds, so adding a third theme later is just a new folder.
  - "Match system" light and dark variants are a later addition. Both themes are dark-first for v1.
- **Accessibility:**
  - text contrast of at least 4.5:1;
  - every control is a real button with a label;
  - "Reduce motion" is respected;
  - avatar states are never shown by colour alone (badges and text accompany them).

---

## 10. Distribution

- **Signing:** a Developer ID Application certificate, a hardened runtime, and notarisation through `notarytool` in CI. Transparent windows need Tauri's `macOSPrivateApi`, which is fine outside the Mac App Store.
- **GitHub Releases:** a universal `.dmg` and `.app.tar.gz`, plus the updater's `latest.json`, signed with the Tauri updater key.
- **Auto-update:** `tauri-plugin-updater` checks GitHub Releases at launch and every 24 h. The user can turn this off.
- **Homebrew:** start with our own tap (`brew install --cask loopstech/tap/agentry`). Submit to `homebrew/cask` once it has enough users. The cask links the `agentry` CLI and runs `agentry hooks uninstall` on removal.
- **Install script:** `curl -fsSL https://<domain>/install.sh | sh`. It downloads the latest release, verifies the checksum, copies the app to `/Applications`, links the CLI, and asks before installing hooks.
- **CI (GitHub Actions):** lint, test, and build on every PR. On a tag: build, sign, notarise, publish the release, and bump the cask.
- **Telemetry:** none.

---

## 11. Phases and acceptance tests

The developer can build and run unit and integration tests in a cloud Linux container. **Each GUI and macOS acceptance check is run by the owner on a Mac.**

**Phase 1: Plumbing**
- Cargo workspace, `agentry-hook`, the `agentry` CLI (`hooks install/uninstall`, `doctor`), and a Tauri shell whose core logs events.
- ✅ Two `claude` sessions in different folders both stream events into the log.
- ✅ With the app quit, Claude Code works with no noticeable delay. The hook exits within 300 ms.
- ✅ Install, then uninstall, leaves `settings.json` exactly as it was, including the user's own hooks.

**Phase 2: Read-only widget (Mono theme)**
- Store, reducer, pill, expanded panel, progress, and nested helpers.
- ✅ "Make a 5-step todo list and work through it slowly" updates the bar and subtitle live, and `Stop` shows "Finished".
- ✅ A prompt that spawns two Explore subagents shows them nested, and they fade out when done.

**Phase 3: Approvals and questions**
- Broker, inline Allow / Deny (with note) / Always allow / Terminal, question chips with "Other", countdown and policy settings.
- ✅ A `Bash` permission appears. Allow runs it, Deny with a note makes Claude acknowledge the note, and Always allow stops future prompts for that rule.
- ✅ An `AskUserQuestion` prompt answered in the widget continues with that answer, using the `updatedInput.answers` mechanism.
- ✅ After the timeout, or when muted, the terminal prompt appears.

**Phase 4: Detail view and jump**
- Transcript view and all `Focus` adapters.
- ✅ ↗ focuses the right iTerm2 or Terminal.app tab, kitty or WezTerm pane, and VS Code or Cursor window. Ghostty and Warp come to the front.

**Phase 5: Crew theme and settings**
- Crew tokens, characters, motion and sounds, plus the settings window, theme switcher and accent picker.
- ✅ Switching theme takes effect instantly and survives a restart.

**Phase 6: Ship**
- First-run flow, launch at login, global shortcut, signing and notarisation, updater, release CI, Homebrew tap, `install.sh`, and README docs.
- ✅ A fresh Mac can install via each of the three channels, gets hooks set up after consent, and auto-updates to a newer test release.

---

## 12. Security and privacy

- **Network:** the server listens on localhost only, requires the token header, and the token file is 0600.
- **Tool input:** never logged to disk. The UI shows it only in memory.
- **No telemetry.** The update check is the only network call.
- **Settings edits:** `~/.claude/settings.json` is only written with a backup and the user's consent.

## 13. Risks and open items

- **Hook API drift.** Keep all payload parsing in `agentry-core`, set a minimum supported Claude Code version, and run `agentry doctor` checks.
- **Editor extensions.** Hooks fire there too, but there's no terminal environment, so jump-to-window only reaches the project window. Confirm in Phase 4.
- **Several sessions in one VS Code window** can't be told apart for focusing. That's acceptable.
- **The terminal freezes during a widget wait.** This is inherent to blocking hooks, and is mitigated by the timeout and policy (§6).
- **Open items for the owner:**
  - the domain for `install.sh` and docs;
  - the Homebrew tap repo name;
  - the bundle ID (e.g. `io.loops-tech.agentry`);
  - the app icon.
