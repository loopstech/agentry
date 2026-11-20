// Glass: a translucent, macOS-style panel with soft highlights, the system
// font, and a glossy coloured orb for every session.

import "./glass.css";
import type { CSSProperties } from "react";
import type { Theme } from "../types";
import { initials } from "../../format";
import { needsUser, type Session } from "../../types";

function state(s: Session) {
  if (needsUser(s)) return "waiting";
  if (s.status === "working") return "working";
  if (s.status === "error") return "error";
  if (s.status === "done") return "done";
  return "ready";
}

function Orb({ session, size }: { session: Session; size: "sm" | "md" }) {
  // Spread sessions around the colour wheel, starting from blue.
  const hue = (210 + session.avatar * 67) % 360;
  return (
    <span
      className={`avatar avatar-${size} avatar-${state(session)}`}
      style={{ "--hue": hue } as CSSProperties}
      aria-hidden="true"
    >
      {size === "md" && <span className="orb-initials">{initials(session.project)}</span>}
    </span>
  );
}

export const glass: Theme = {
  id: "glass",
  name: "Glass",
  description: "Translucent and glossy, like macOS, with a coloured orb per session.",
  className: "theme-glass",
  defaultAccent: "#0a84ff",
  accents: ["#0a84ff", "#bf5af2", "#ff375f", "#30d158", "#ff9f0a"],

  Avatar: Orb,

  Progress: ({ done, total }) => (
    <div
      className="glass-progress"
      role="progressbar"
      aria-valuemin={0}
      aria-valuemax={total}
      aria-valuenow={done}
      aria-label={`${done} of ${total} tasks done`}
    >
      <div className="glass-progress-fill" style={{ width: `${total ? (done / total) * 100 : 0}%` }} />
    </div>
  ),

  strings: {
    pillSummary: (c) => {
      const parts = [c.working && `${c.working} working`, c.ready && `${c.ready} ready`, c.done && `${c.done} done`].filter(
        Boolean,
      ) as string[];
      if (c.needs) return { strong: `${c.needs} need${c.needs === 1 ? "s" : ""} you`, rest: parts.join(" · ") };
      if (parts.length) return { strong: parts[0], rest: parts.slice(1).join(" · ") };
      return { strong: "Agentry", rest: "no sessions" };
    },
    emptyTitle: "No Sessions",
    emptyHint: "Start claude in a terminal or your editor and it shows up here.",
    waitingInTerminal: "Waiting for you in the terminal",
    allow: "Allow",
    deny: "Don't Allow…",
    denyConfirm: "Don't Allow",
    denyAndStop: "Don't Allow & Stop",
    notePlaceholder: "Tell Claude why (optional)",
    send: "Send",
    skip: "Skip",
    useTerminal: "Use Terminal",
    helpersHeading: null,
  },
};
