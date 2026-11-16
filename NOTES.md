# Notes

Facts checked against real Claude Code runs, and the places where they differ from or add to PLAN.md.

## Claude Code 2.1.325, observed in Phase 1 (2026-11-16)

These come from real `claude -p` sessions with Agentry's hooks installed. Payload field names were logged with `RUST_LOG=debug` on the dev server; names only, never values.

| Event | Fields |
|---|---|
| SessionStart | session_id, transcript_path, cwd, scratchpad_dir, hook_event_name, source |
| UserPromptSubmit | + prompt_id, permission_mode, prompt |
| PermissionRequest | + prompt_id, permission_mode, effort, tool_name, tool_input, permission_suggestions |
| PostToolUse | + prompt_id, permission_mode, effort, tool_name, tool_input, tool_response, tool_use_id, duration_ms (+ agent_id, agent_type inside subagents) |
| SubagentStart | + prompt_id, agent_id, agent_type |
| SubagentStop | + permission_mode, effort, stop_hook_active, agent_transcript_path, last_assistant_message, background_tasks, session_crons |
| Stop | + stop_hook_active, last_assistant_message, background_tasks, session_crons |
| SessionEnd | + prompt_id, reason |

- **Task tracking.** This version tracks tasks with `TaskCreate`/`TaskUpdate`, not `TodoWrite`. The `TaskCreated` and `TaskCompleted` hooks fire as tasks change. Phase 2 must support both.
- **Subagents.** Their events carry the **parent's** `session_id` plus `agent_id` and `agent_type`, so nesting them under the parent card is simple. The parent then sees its own `PostToolUse` for the `Agent` tool.
- **When `PermissionRequest` fires.** Only when Claude Code would actually ask. A command pre-allowed with `--allowedTools` produced no `PermissionRequest`; a Read outside the project did.
- **Session title.** `SessionStart` has no `session_title` field when the session has no title, so the fallback title is needed.
- **Async hooks.** `"async": true` hooks work, and the sessions showed no delay from them.
- **Fail-open.** With the app stopped, the hook takes about 6 ms per run including process start, and `claude -p` completes normally.
- **Task payloads** (captured in Phase 2; full sanitised sessions in `crates/agentry-server/tests/fixtures/`):
  - `TaskCreated` / `TaskCompleted` have `task_id`, `task_subject` and `task_description`. They carry no status or activeForm.
  - `PostToolUse` on `TaskCreate` has `tool_input.{subject, description, activeForm}` and `tool_response.task.id`.
  - `PostToolUse` on `TaskUpdate` has `tool_input.{taskId, status}`, and the response includes a `statusChange`.
  - The model can move tasks backwards (in_progress → pending → in_progress), so the store tracks each task's state rather than counting events.
- **Task tools in `-p` mode.** `TaskCreate`/`TaskUpdate` were only offered when `TodoWrite` was in `--allowedTools`; without it the model reported the tools as unavailable. This only matters for scripted test runs.
- **Not yet observed:**
  - `Notification` and `StopFailure`;
  - `PostToolUseFailure`.

## Approvals and questions, verified in Phase 3 (Claude Code 2.1.325)

All of these were checked against real sessions. Interactive sessions were driven in tmux, with real mouse clicks on the widget's buttons on a virtual display.

- **The terminal is not frozen.** In interactive sessions Claude Code shows its own "Do you want to proceed?" dialog **while** the `PermissionRequest` hook is still waiting.
  - Answering in the widget closes the terminal dialog; Claude Code logs "Allowed by PermissionRequest hook".
  - Answering in the terminal makes Claude Code abandon the hook. The server notices the dropped connection and clears the card at once.
  - So PLAN.md §6's "terminal is frozen" trade-off doesn't apply to interactive sessions. Only `claude -p` really waits on the widget.
- **Allow** runs the tool.
- **Deny with a note:** the note reaches Claude as the denial message, and Claude follows it. For example, it used Write instead of a denied `touch`, and it kept a file the user said they needed.
- **Always allow:** sending back `permission_suggestions` as `updatedPermissions` makes Claude Code save the rule. It wrote `Bash(python3 -c 'print\(6*7\)')` into `.claude/settings.local.json`, and the repeat command raised no `PermissionRequest`.
  - Suggestions aren't always rules. For `touch` they were `addDirectories` plus `setMode: acceptEdits` for the session, and the button label describes exactly what will be applied.
- **Answering questions:** `PreToolUse` on `AskUserQuestion`, returning `allow` with `updatedInput` = the original questions plus `answers`, works as documented. Claude Code shows "User answered Claude's questions: · Which fruit do you want? → Cherry" and carries on. The deny-with-answer fallback isn't needed.
- **`AskUserQuestion` in `-p` mode:** the tool isn't offered at all in non-interactive runs.
- **Unnamed internal agents:** after each turn, interactive Claude Code fires `SubagentStop` with an **empty** `agent_type` and no matching `SubagentStart`. These are ignored; only named subagents become helpers.
- **Hook killed:** when Claude Code kills a waiting hook (e.g. the user answers in the terminal or interrupts), axum drops the request handler, and a guard hands the request back and clears the card.

## Decisions and deviations in Phase 1

- **App bundle ID.** It's `io.loopstech.agentry` as a placeholder; this is still an open item for the owner. Changing it later moves the app's macOS preferences, so decide before the first public release.
- **Placeholder icon.** Generated in `app/src-tauri/icons/` until a real one exists.
- **Port.** The app picks a random free port and writes it to `~/.agentry/port`, so there's no fixed port 7777 to collide with. A second app instance refuses to start if the first is answering.
- **Stale port file after a crash.** If the app is killed (SIGKILL/SIGTERM), the port file stays behind. That's harmless: the connection is refused immediately and the hook exits. `agentry doctor` reports it as a warning. The dev server cleans up on SIGTERM.
- **Bundling the hook and CLI inside `Agentry.app`.** This (Tauri `externalBin`) is deferred to Phase 6. Until then, `agentry hooks install` points at the `agentry-hook` built next to the `agentry` command (`target/release/`).
- **Older Claude Code versions.** It isn't yet known how they react to hook events they don't recognise (e.g. `TaskCreated`). Before Phase 6, find the minimum version for each event, and make `install` register only what the installed version supports.

## Decisions and deviations in Phase 2

- **Where the store lives.** The session store (`crates/agentry-server/src/store.rs`) is shared by the app, the dev server and `agentry status` (`GET /sessions`). Every change is pushed to the UI as a complete snapshot, which is simple and cheap at a handful of sessions.
- **Title fallback** (when Claude Code gives no title): the first prompt's first clause, at most 6 words.
  - Leading filler such as "can you" or "please" is dropped, and so are trailing words like "to" or "the".
  - `/rename <name>` typed in a session also sets the title.
- **Avatar initials.** They split on case and digit boundaries ("projA" becomes "PA"). For a single word they use the first letter plus the next consonant ("web" becomes "WB", "docs" becomes "DC").
- **Anchoring the window.** The app keeps the island's top-centre anchor itself, instead of re-reading the window's geometry on every resize. Re-reading made the window drift sideways when resizes arrived in quick succession (seen on Linux/X11). Moves the app causes itself are ignored; only user drags change the anchor, which is saved to `~/.agentry/window.json`.
- **Dock icon and quitting.** There's still a Dock icon, so the app can be quit normally (⌘Q). It becomes a menu bar–only app with a menu bar icon in Phase 6.
- **Full-screen apps.** `visibleOnAllWorkspaces` covers Spaces. Showing over full-screen apps needs an extra macOS window setting; that's planned for Phase 6.
- **First run of a new binary on macOS** takes a few hundred ms, because macOS scans a freshly built, unsigned binary the first time it runs. CI caught this in Phase 1, and the timing tests now warm the binary up first. Signed and notarised builds (Phase 6) should be checked for this too.
- **Not done yet:** backfilling sessions that were already running when the app started. This moves to Phase 4, alongside transcript reading.

## Decisions and deviations in Phase 3

- **Settings** live in `~/.agentry/config.json`. The server re-reads them on every request, so changes apply immediately. They can be edited with `agentry config` until the Phase 5 Settings window exists.
- **Mute is "quiet mode".** It will also silence sounds in Phase 5, and every prompt goes straight to the terminal.
- **The panel opens by itself** when a request arrives (`approvals.autoOpen`, default on). It doesn't take keyboard focus until you click it.
- **Wording.** The countdown reads "widget hands over in 0:48", and the card says "You can also answer in the terminal", which matches what actually happens.
- **`POST /requests/{id}`** answers a request exactly as the buttons do. Tests and scripts use it; it requires the token like every other route.
- **Not done:** keyboard shortcuts for Allow and Deny (⌘↵ / ⌘⌫). They need a global shortcut, so they move to Phase 6.

## Phase 4 findings (Claude Code 2.1.325)

- **Transcripts** contain more than messages. Lines Agentry uses:
  - `user`: a string prompt, or `tool_result` items with `is_error`;
  - `assistant`: `text` and `tool_use` items;
  - `ai-title`: Claude Code's generated session title, e.g. "Plan.md bread-baking steps";
  - `custom-title`: written by `/rename`.
  - Ignored: `attachment`, `system`, `file-history-*`, `mode`, `permission-mode`, `last-prompt`, `tools-used`, `cost-state`, `atis-latch`.
- **`/rename` fires no hook.** It only writes a `custom-title` line. So the server re-reads live sessions' transcripts (incrementally) on its 5-second tick, and a rename shows within a few seconds.
- **Title priority:** `/rename` (or a custom session title), then Claude Code's `ai-title`, then a title made from the prompt, then "New session". Later prompts never replace a better title.
- **Sessions running before the app started** are picked up from transcripts changed in the last 30 minutes. The title comes from the transcript and the card says "Active before Agentry started".
  - A transcript doesn't record that a session ended, so a session that exited within the last 30 minutes also appears. It drops off 30 minutes after its last activity, or becomes a normal card as soon as it sends a live event.
  - Its window isn't known until that first live event (the hook records the terminal), and the ↗ button says so.

## Decisions and deviations in Phase 4

- **The transcript reader** (`crates/agentry-server/src/transcript.rs`) reads incrementally from a remembered offset.
  - On first open it reads at most the last 4 MB, and at most 8 MB per poll.
  - It keeps the last 200 steps.
  - It only opens `.jsonl` files.
  - `cargo run -p agentry-server --example transcript -- <file>` prints what it reads.
- **The detail view** polls every 1.5 s while open, and follows new steps unless you've scrolled up. Claude's markdown is flattened to plain text.
- **Jump to window** (`app/src-tauri/src/focus.rs`):

  | Where the session runs | What ↗ does |
  |---|---|
  | iTerm2 | Selects the exact session (AppleScript, `ITERM_SESSION_ID`) |
  | Terminal.app | Selects the exact tab (AppleScript, matching the tty) |
  | tmux | Selects the exact pane (`TMUX_PANE`), then brings the outer terminal app forward |
  | kitty | Focuses the window, if kitty's remote control is on |
  | WezTerm | Activates the pane |
  | Ghostty, Warp | Brings the app forward |
  | VS Code, Cursor, Windsurf, Insiders, VSCodium | Opens `<scheme>://file/<cwd>`: the project window, not the specific Claude panel |

  - Every value put into a script or command is checked against a strict character set first.
  - The tmux jump was verified for real, both in a test and through the GUI. The AppleScript paths need a Mac.
- **Use terminal** on a request card hands the request back and also jumps to the session.
- **macOS Automation permission.** The first jump to iTerm2 or Terminal triggers macOS's prompt to allow controlling that app. The Phase 6 first-run screen should explain this.
