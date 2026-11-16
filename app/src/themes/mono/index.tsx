// Mono: black, monochrome, one accent colour (design direction D).

import "./mono.css";
import type { Theme } from "../types";
import { initials, plural } from "../../format";
import { needsUser } from "../../types";

export const mono: Theme = {
  id: "mono",
  name: "Mono",
  className: "theme-mono",

  Avatar: ({ session, size }) => {
    const state = needsUser(session)
      ? "needs"
      : session.status === "working"
        ? "working"
        : session.status === "error"
          ? "error"
          : "quiet";
    return (
      <span className={`avatar avatar-${size} avatar-${state}`} aria-hidden="true">
        {initials(session.project)}
      </span>
    );
  },

  Progress: ({ done, total }) => (
    <div
      className="progress"
      role="progressbar"
      aria-valuemin={0}
      aria-valuemax={total}
      aria-valuenow={done}
      aria-label={`${done} of ${total} tasks done`}
    >
      <div className="progress-fill" style={{ width: `${total ? (done / total) * 100 : 0}%` }} />
    </div>
  ),

  strings: {
    pillSummary: (c) => {
      const parts = [
        c.working && `${c.working} working`,
        c.ready && `${c.ready} ready`,
        c.done && `${c.done} done`,
      ].filter(Boolean) as string[];
      if (c.needs) return { strong: `${c.needs} need${c.needs === 1 ? "s" : ""} you`, rest: parts.join(" · ") };
      if (parts.length) return { strong: parts[0], rest: parts.slice(1).join(" · ") };
      return { strong: "Agentry", rest: "no sessions" };
    },
    emptyTitle: "No Claude Code sessions yet",
    emptyHint: "Start claude in a terminal or your editor and it shows up here.",
    waitingInTerminal: "Waiting for you in the terminal",
  },
};

export const sessionCount = (n: number) => plural(n, "session", "sessions");
