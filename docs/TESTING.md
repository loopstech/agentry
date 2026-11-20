# Manual test checklists

Things the automated tests can't cover: how the widget behaves on a real Mac,
with real terminals and a real Claude Code. Run through the relevant list
before a release, or after changing that area. Set up a build from source
first ([README → Building from source](../README.md#building-from-source)).

Each list says what you should see. If something differs, note your macOS
version, terminal and editor in the issue.

## Notch mode (MacBooks with a notch)

1. **Attached.** The island grows out of the notch instead of floating below the menu bar:
   - collapsed, it's a black shape fused to the notch, with session avatars on the left and the most important count on the right (orange = needs you, ringed = working, grey = ready);
   - with no sessions it's exactly the notch's size and disappears into it. Clicking the notch still opens the panel.
2. **Expanded.** Click it, or press ⌥⌘A. The panel drops down from the notch, with a black band under the notch so it blends in (Crew's navy starts below it).
3. **Setting.** Settings → Appearance → *Attach to the notch* (on by default). Turn it off and the island goes back to floating below the menu bar, draggable, where you last left it.
4. **Other apps in front.** Click into VS Code (or any app), make an app full screen, switch desktops, and turn Stage Manager on if you use it. The island stays at the notch the whole time.
5. **External screens.** Drag the island to a screen without a notch (with the setting off), turn the setting back on, and it stays on that screen, floating. Notch mode only applies on the screen that has the notch.

Does the shape line up exactly with your notch (width, height, corners)? Does it cover menu bar items you use next to the notch?

## Menu bar, shortcut and release build

1. **Menu bar.** There's no Dock icon. A menu bar icon offers Show or Hide, Settings…, Quiet Mode and Quit. "Show a Dock icon" in Settings brings the Dock icon back.
2. **Shortcut.** Press **⌥⌘A** in any app: the panel opens, and pressing it again closes it. With a request waiting, ⌘↵ allows it and ⌘⌫ denies it. Change or turn off the shortcut in Settings.
3. **Full-screen apps.** Put a terminal in full screen. The island should still float on top.
4. **Only one Agentry.** Launching it again just opens the panel of the running one.
5. **Welcome.** Remove `"onboarded"` from `~/.agentry/config.json` and relaunch. The welcome screen appears and walks through connecting, installing the command and picking a theme.
6. **Open at login.** Toggle it in Settings (General), log out and back in.
7. **Release build.** Follow [RELEASING.md](../RELEASING.md) → *Building locally*. The app should contain `Contents/MacOS/agentry-hook` and `agentry`, and "Connect to Claude Code" should point at those.

## Settings, themes and sounds

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

## Recent steps and jumping to a session

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

Note which terminal and editor you used, and whether ↗ found the right tab: those AppleScript paths can't be tested automatically.

## What the approval card shows

1. **Long commands.** Ask Claude to run a command of 40 or more lines (for example *"run a bash script that echoes the numbers 1 to 40, one echo per line"*). The card shows every line, with "40 lines · scroll to read it all" under the box; scroll to the end.
2. **Padding.** Ask for a command with dozens of blank lines in the middle. They're folded into one "⟨N blank lines⟩" line, so what comes after is still visible.
3. **Hidden characters.** Ask Claude to `echo` a string containing a right-to-left override (U+202E) or a zero-width space (U+200B). The card shows them as `⟨U+202E⟩` and warns about invisible or text-direction characters.
4. **Too long to show.** Ask Claude to Write a file of more than 100,000 characters. The card says it's too long to show in full, Allow and "Always allow" are disabled, and ⌘↵ does nothing but explain why. Deny and Use terminal still work.
5. **MultiEdit.** Every edit appears, headed "@@ edit 1 of N @@".

## Chat safety

1. With chat on for a session in tmux, iTerm2 or WezTerm, suspend Claude with **Ctrl-Z**. Sending a message is refused ("isn't running in this session's terminal") and nothing is typed into the shell. Run `fg` and sending works again.
2. Quit Claude and start a Node program in the same terminal (for example `node -e "setInterval(()=>{},1000)"`). Sending is refused.

## Approvals and questions

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

The same settings from the command line:

```sh
./target/release/agentry config                          # list everything
./target/release/agentry config set approvals.timeout 120
./target/release/agentry config set approvals.mode widget_when_visible
./target/release/agentry config set approvals.tools Bash,Write,Edit
```

## Live progress

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

These need a real Mac:
- Is the panel's background transparent around the rounded corners, with a soft shadow?
- Does the pill stay on top of other windows?
- Does it follow you across desktops (Spaces)?
- Does dragging work, and does the position survive a restart?

## Agentry stays out of the way

- **Quitting the app changes nothing.** Quit with ⌘Q and keep using Claude Code; there's no delay, because `agentry-hook` exits within milliseconds when the app is down.
- **Uninstall leaves settings intact.** Run `./target/release/agentry hooks uninstall`. Your `~/.claude/settings.json` should match the backup made before install, including your own hooks:

  ```sh
  diff <(jq -S . ~/.claude/settings.json) <(jq -S . "$(ls -1 ~/.claude/settings.json.agentry-backup-* | head -1)")
  ```
