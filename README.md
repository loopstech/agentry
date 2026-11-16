# Agentry

A small always-on-top macOS widget that shows every running Claude Code session. You can approve permissions and answer Claude's questions from it, and jump back to the session's window.

> **Status: Phase 1 (plumbing).** There's no widget yet. You get the hook program, the `agentry` command, and an app window that logs every hook event live. See [PLAN.md](PLAN.md) for the roadmap.

## How it works

Claude Code runs `agentry-hook` for each hook event. The hook forwards the event to the Agentry app over `127.0.0.1`, using a token stored in `~/.agentry/token`.

If the app isn't running, the hook exits immediately and silently, so Claude Code behaves exactly as if Agentry weren't installed.

| Piece | What it is |
|---|---|
| `agentry-hook` | A tiny Rust program (~0.5 MB). Claude Code runs it for each event. |
| `agentry` | The command-line tool: install and remove hooks, `status`, `doctor`. |
| Agentry app | Tauri app. Runs the local server; shows sessions. |
| `agentry-devserver` | The app's server without a window, for development. |

## Testing Phase 1 on a Mac

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

Now run the three Phase 1 acceptance checks:

1. **Two sessions stream in.** Start `claude` in two different folders and give each a task. Events from both appear in the app window and in the `pnpm tauri dev` terminal.
2. **Quitting the app changes nothing.** Quit the app with ⌘Q and keep using Claude Code. There should be no delay and no errors, because `agentry-hook` exits within milliseconds when the app is down.
3. **Uninstall leaves settings intact.** Run `./target/release/agentry hooks uninstall`. Your `~/.claude/settings.json` should match the backup made before install, including any hooks you had yourself:

   ```sh
   diff <(jq -S . ~/.claude/settings.json) <(jq -S . "$(ls -1 ~/.claude/settings.json.agentry-backup-* | head -1)")
   ```

## Commands

```
agentry hooks install [--project [DIR]] [--hook-path PATH]
agentry hooks uninstall [--project [DIR]]
agentry status
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
