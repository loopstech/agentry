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
- **Dock icon and quitting.** Superseded in Phase 6: Agentry is now a menu bar app (no Dock icon unless you turn it on), quit from the menu bar icon.
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

## Decisions and deviations in Phase 5

- **Themes** live in `app/src/themes/<name>/`: tokens and styles (CSS), avatar art, progress style, helper icon and wording. The components are shared. Adding a theme means a new folder plus one line in `themes/index.ts`.
  - **Mono:** initials, hairline progress, monochrome with one accent.
  - **Crew:** a character per session (robot, blob, ghost or triangle, in two colour sets), chosen from the session's stable avatar number. It has a mood for each status (busy, waiting, ready, done, error), with a "!" or "?" badge when waiting on a permission or question. It also has step dots (scaled to 12 for long lists), a "Helpers" box, chunky buttons, a bounce while working and a pop-in panel. All motion respects "Reduce motion".
- **Settings** are in `~/.agentry/config.json`, which now has `appearance` (theme, accent, hoverToOpen) and `sounds` (enabled, volume) sections.
  - The app watches the file every second, so edits from the Settings window, `agentry config` or by hand reach the island live.
  - Hand-edited values are cleaned up on load: volume is clamped, and an invalid accent colour falls back to the theme default.
- **The Settings window** (`#settings`, opened by the gear) uses only the current theme's colour tokens (`.tokens-<name>`). If it used the full theme class, Crew's styles would leak into the Mono preview card.
- **Sounds** use macOS system sounds through `afplay`:

  | Theme | Needs you | Finished |
  |---|---|---|
  | Mono | Tink | Purr |
  | Crew | Pop | Hero |

  - They're played natively, not with web audio, because browsers block web audio until the page has been clicked.
  - One sound per change at most; sessions picked up at launch don't trigger sounds.
  - Quiet mode silences them. On Linux they're logged instead.
- **"MCP tools"** in the tool list matches every `mcp__*` tool.
- **Bug found while testing:** move events from the Settings window were being treated as the island being dragged, so the island reopened where Settings had been. Only the island's own moves count now.
- **Not done:** titles via Haiku (dropped earlier in favour of Claude Code's own titles); light variants of the themes (later, as planned).

## Decisions and deviations in Phase 6

- **Menu bar app.** `Info.plist` sets `LSUIElement`, so there's no Dock icon flash at launch; "Show a Dock icon" switches the activation policy at runtime. The menu bar icon is a template image (it follows light and dark menu bars) with Show or Hide, Settings…, Quiet Mode (kept in sync) and Quit.
- **Over full-screen apps.** tao only sets CanJoinAllSpaces, so `macos.rs` also sets FullScreenAuxiliary, Stationary and IgnoresCycle through `objc2`, and raises the window level to NSStatusWindowLevel. This code only compiles on macOS; the macOS CI job builds it, but whether it shows over full-screen apps still needs checking on a real Mac.
- **Global shortcut:** ⌥⌘A by default (`general.shortcut`, changeable or "off"). It opens and closes the panel and focuses it, so ⌘↵ / ⌘⌫ (allow / deny the top request) work without the mouse. Those two are panel-only, so they never take over the keys in your terminal.
- **Single instance.** Launching Agentry again opens the running one's panel. On Linux this needs a D-Bus session (desktops have one; the bare test container didn't). macOS doesn't use D-Bus.
- **Launch at login** uses tauri-plugin-autostart (a LaunchAgent on macOS). It's the operating system's own setting, not something stored in `config.json`.
- **Updates** use tauri-plugin-updater with the GitHub Releases feed (`latest.json`).
  - The plugin is only registered when the build has an updater config, which the release workflow adds once the key exists. Local builds say "Updates aren't set up in this build".
  - Updates install automatically when allowed, and the new version runs after a restart (Settings → Updates shows "Restart to update").
- **Packaging.** `agentry-hook` and `agentry` are bundled as Tauri sidecars, so they sit next to the app binary.
  - They're configured in `tauri.release.json`, so dev builds and CI tests don't need them.
  - `scripts/build-sidecars.sh universal` makes Apple Silicon + Intel binaries with `lipo`.
  - Verified with a Linux `.deb`: the app, `agentry` and `agentry-hook` all landed in `/usr/bin`. A fresh user then went through the welcome screen and "Connect to Claude Code" (hooks pointed at the bundled `/usr/bin/agentry-hook`), the command install (`/usr/local/bin/agentry` linked) and open at login (autostart entry written).
- **Release pipeline:**
  - `release.yml` (on a `v*` tag): checks that the tag matches the app version, builds the universal sidecars, then signs and notarises only when the Apple secrets exist. It enables updates only when the key exists, drafts the release, and uploads `Agentry.dmg` and `SHA256SUMS` under stable names.
  - `homebrew.yml` (on publish): renders the cask from `packaging/homebrew/agentry.rb` and pushes it to the tap. The cask's uninstall step removes the hooks.
  - Neither workflow has run yet (that needs a tag), and the actionlint binary couldn't be run here. Expect to fix small things on the first real release.
- **Install script:** `scripts/install.sh` checks the download's SHA-256, installs to /Applications (or ~/Applications), links the command, and asks before connecting hooks (reading from /dev/tty when piped).
  - `scripts/tests/install-test.sh` runs it against stand-ins for the macOS tools: a fresh install, a re-install, and a tampered download refused. CI runs it with shellcheck.
- **Still open for the owner:** the bundle ID (`io.loopstech.agentry` is a placeholder), the app icon (placeholder), the tap repository, and the secrets in RELEASING.md. Also still unknown: the minimum Claude Code version for each hook event (see Phase 1 notes).

## Notch mode (added after Phase 6)

- **On by default** (`appearance.notch`). On a screen with a notch the island is attached to it, flush with the top of the screen, above the menu bar. Elsewhere, or with the setting off, it floats below the menu bar as before.
- **Measuring the notch** (`macos::notches`, at launch on the main thread): a screen has a notch when `NSScreen.safeAreaInsets.top > 0` and it reports `auxiliaryTopLeftArea` / `auxiliaryTopRightArea` (macOS 12+).
  - The notch's width is the gap between those two areas, and its height is the top inset.
  - The position is taken as centred on the screen (true of every MacBook), which avoids depending on whether AppKit reports those areas in screen or global coordinates.
- **Which notch:** the one on the screen the island would otherwise be on (its saved spot, or the main screen). `pick_notch` is pure and unit-tested.
- **In notch mode the island can't be dragged,** and moves aren't saved. Turning the setting off restores the saved free position.
- **Look.**
  - Collapsed, it's a black shape with concave "shoulders" and bottom corners rounded at 14 pt.
  - Each "ear" either side of the notch is 72 pt when sessions exist. The left ear has up to 3 avatars; the right ear has the most important count.
  - Expanded, the panel has square top corners and a black inset collar the notch's height, so Crew's navy starts below the notch.
- **Staying in front of other apps** (reported: the island vanished when VS Code came to the front).
  - The level is now `NSMainMenuWindowLevel + 3` (27), as other notch apps use, instead of the status level (25). The Space behaviour clears bits AppKit treats as conflicting with joining every Space (`island_behaviour`, unit-tested).
  - `macos::float_everywhere` also watches `NSWorkspaceDidActivateApplicationNotification` and `NSWorkspaceActiveSpaceDidChangeNotification`. On each one it re-applies the level and behaviour and calls `orderFrontRegardless` if the island is visible. It logs a warning if something had reset them.
  - With a Dock icon, the behaviour is set while Agentry is briefly an accessory app, then the policy goes back to regular (Electron does the same for windows that float over full-screen apps). The policy only changes when the setting does.
  - That alone didn't cover the reported case: VS Code full screen with Stage Manager on. In both, macOS treats an ordinary window from another app like any other window.
  - **The island is now a non-activating `NSPanel`**, via [`tauri-nspanel`](https://github.com/ahkohd/tauri-nspanel), the usual way to do this in Tauri. Its full-screen example is this case. The crate swaps the window's class in place (`object_setClass`) and leaves tao's delegate alone, so move and focus events still arrive.
  - `can_become_key_window` keeps text fields (deny notes, answers) and ⌘↵ / ⌘⌫ working. `hidesOnDeactivate` is forced off, since panels default to hiding when their app isn't active.
  - The shortcut and relaunching use `show_and_make_key` instead of activating Agentry, so the full-screen app you're in stays in front.
  - Not yet confirmed on a Mac. Watch for: text input in the panel, hover-to-open while another app is active (WebKit may not report hover in an inactive app), and the panel staying put through full screen, Stage Manager and desktop switches.
- **Smooth expand and collapse** (reported: clicking the island to expand glitched).
  - Cause: every switch resized the native window to fit the new content. The page drew the panel in the old small window first, then the window was resized and moved in separate steps. None of that was in step with the web view, so it jumped for a frame or two, and the pill was swapped for the panel with no transition.
  - Now the window stays the size of the open panel (`set_panel_size` is only called when that size changes). One `.island-shape` carries the surface, corners, shadow and notch collar, and animates `width`/`height`/`border-radius`/colour between the measured pill and panel sizes (440 ms open, 360 ms close, `cubic-bezier(0.32, 0.72, 0, 1)`). The content is clipped to it and fades in after a switch. Content changes inside the open panel (a row expanding, a new session) animate the same way. Reduced motion turns the animation off.
  - Click-through: the transparent rest of the window must not block the menu bar or apps. The UI reports the shape's rectangle (`set_hit_area`), and `hit::watch` polls the cursor every 25 ms, calling `set_ignore_cursor_events` when it crosses the edge (`wants_mouse`, unit-tested).
  - Checked by freezing the CSS animations at set times in headless Chromium (notch and floating, both themes). Not yet seen on a Mac.
  - Known gap: in floating mode, the window's height is fitted to the screen when it's placed, not after a drag. A panel dragged low can run past the bottom of the screen.
- **Checking macOS code from Linux:** `cargo check --target aarch64-apple-darwin` works with a stand-in C compiler for `objc2-exception-helper`'s build script (set `CC_aarch64_apple_darwin` and `AR_aarch64_apple_darwin` to no-op scripts). Nothing is linked, so it only type-checks. The notch code and the existing `objc2` code pass `cargo clippy` for that target.
- **Not handled yet:**
  - **Screens connected or disconnected while Agentry runs:** notches are measured at launch. Relaunch, or toggle the setting, to pick up a change.
  - **Other "notch" apps** (e.g. NotchNook) want the same space; turn notch mode off if you use one.
  - **Status icons right next to the notch** can be covered by the ears.
