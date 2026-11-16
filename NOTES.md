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
- **Not yet observed:**
  - `Notification` and `StopFailure`, which need interactive sessions or errors;
  - `PostToolUseFailure`;
  - `TaskCreated`/`TaskCompleted` fields, which fired but weren't captured with field logging on;
  - `AskUserQuestion` through PreToolUse, which needs an interactive session.

## Decisions and deviations in Phase 1

- **App bundle ID.** It's `io.loopstech.agentry` as a placeholder; this is still an open item for the owner. Changing it later moves the app's macOS preferences, so decide before the first public release.
- **Placeholder icon.** Generated in `app/src-tauri/icons/` until a real one exists.
- **Port.** The app picks a random free port and writes it to `~/.agentry/port`, so there's no fixed port 7777 to collide with. A second app instance refuses to start if the first is answering.
- **Stale port file after a crash.** If the app is killed (SIGKILL/SIGTERM), the port file stays behind. That's harmless: the connection is refused immediately and the hook exits. `agentry doctor` reports it as a warning. The dev server cleans up on SIGTERM.
- **Bundling the hook and CLI inside `Agentry.app`.** This (Tauri `externalBin`) is deferred to Phase 6. Until then, `agentry hooks install` points at the `agentry-hook` built next to the `agentry` command (`target/release/`).
- **Older Claude Code versions.** It isn't yet known how they react to hook events they don't recognise (e.g. `TaskCreated`). Before Phase 6, find the minimum version for each event, and make `install` register only what the installed version supports.
