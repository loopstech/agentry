// Terminal: green phosphor on black, monospace everywhere, and an ASCII face
// for every session.

import "@fontsource/geist-mono/600.css";
import "@fontsource/geist-mono/700.css";
import "./terminal.css";
import type { Theme } from "../types";
import { needsUser, type Session } from "../../types";

type Mood = "working" | "waiting" | "ready" | "done" | "error";

/** Each session gets its own brackets, so faces stay tell-apart-able. */
const BRACKETS = ["[]", "()", "{}", "<>", "||", "//"];

/** Eyes and mouth per mood. */
const FACES: Record<Mood, { eyes: [string, string]; mouth: string }> = {
  working: { eyes: ["o", "o"], mouth: "_" },
  waiting: { eyes: ["O", "O"], mouth: "o" },
  ready: { eyes: ["^", "^"], mouth: "_" },
  done: { eyes: ["-", "-"], mouth: "_" },
  error: { eyes: ["x", "x"], mouth: "~" },
};

function mood(s: Session): Mood {
  if (needsUser(s)) return "waiting";
  if (s.status === "working") return "working";
  if (s.status === "error") return "error";
  if (s.status === "done") return "done";
  return "ready";
}

function Face({ session, size }: { session: Session; size: "sm" | "md" }) {
  const m = mood(session);
  const { eyes, mouth } = FACES[m];
  const [open, close] = BRACKETS[session.avatar % BRACKETS.length];
  // The pill only has room for the eyes.
  const text = size === "sm" ? eyes.join("") : `${open}${eyes[0]}${mouth}${eyes[1]}${close}`;
  return (
    <span className={`avatar avatar-${size} avatar-${m}`} aria-hidden="true">
      {text}
    </span>
  );
}

const BAR = 12;

export const terminal: Theme = {
  id: "terminal",
  name: "Terminal",
  description: "Green phosphor on black, monospace, an ASCII face per session.",
  className: "theme-terminal",
  defaultAccent: "#ffb000",
  accents: ["#ffb000", "#33ff66", "#4fd6ff", "#ff5fd2", "#f2f2f2"],

  Avatar: Face,

  // [#####-------] 5/12
  Progress: ({ done, total }) => {
    const filled = total ? Math.round((done / total) * BAR) : 0;
    return (
      <div
        className="ascii-progress"
        role="progressbar"
        aria-valuemin={0}
        aria-valuemax={total}
        aria-valuenow={done}
        aria-label={`${done} of ${total} tasks done`}
      >
        <span aria-hidden="true">
          [<span className="ascii-progress-done">{"#".repeat(filled)}</span>
          {"-".repeat(BAR - filled)}]
        </span>
      </div>
    );
  },

  HelperIcon: ({ done }) => (
    <span className={`prompt-mark${done ? " prompt-mark-done" : ""}`} aria-hidden="true">
      {done ? "✓" : ">"}
    </span>
  ),

  strings: {
    pillSummary: (c) => {
      const parts = [c.working && `${c.working} running`, c.ready && `${c.ready} idle`, c.done && `${c.done} exited`].filter(
        Boolean,
      ) as string[];
      if (c.needs) return { strong: `${c.needs} waiting on you`, rest: parts.join(" · ") };
      if (parts.length) return { strong: parts[0], rest: parts.slice(1).join(" · ") };
      return { strong: "agentry", rest: "no sessions" };
    },
    emptyTitle: "no sessions running",
    emptyHint: "$ claude    # start one in a terminal or your editor",
    waitingInTerminal: "waiting for you in the terminal",
    allow: "allow",
    deny: "deny…",
    denyConfirm: "deny",
    denyAndStop: "deny + stop",
    notePlaceholder: "# tell claude why (optional)",
    send: "send",
    skip: "skip",
    useTerminal: "use terminal",
    helpersHeading: "subagents",
  },
};
