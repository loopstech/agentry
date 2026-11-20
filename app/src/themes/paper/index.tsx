// Paper: a light theme. Warm off-white, ink-black text, a serif for titles,
// and a hand-drawn doodle for every session.

import "@fontsource/fraunces/600.css";
import "./paper.css";
import type { Theme } from "../types";
import { needsUser, type Session } from "../../types";

type Mood = "working" | "waiting" | "ready" | "done" | "error";

const INK = "#1f1b16";
/** Highlighter colours for the doodles' heads. */
const COLOURS = ["#ffe066", "#a5d8ff", "#b2f2bb", "#ffc9c9", "#d0bfff", "#ffd8a8"];
type Extra = "none" | "tuft" | "glasses" | "ears";
const EXTRAS: Extra[] = ["none", "tuft", "glasses", "ears"];

/** A slightly lopsided circle, as if drawn by hand (56×56 box). */
const HEAD = "M28 8.5c11.2-.3 20.2 8.6 19.8 20.2-.4 11.4-9.3 20.1-20.4 19.8C16.6 48.2 7.7 39.6 8.3 28 8.9 16.8 17.4 8.8 28 8.5z";
const EYE_Y = 27;
const LX = 21;
const RX = 35;
const MOUTH_Y = 37;

const line = { fill: "none", stroke: INK, strokeWidth: 2.2, strokeLinecap: "round" as const, strokeLinejoin: "round" as const };

function Extras({ kind }: { kind: Extra }) {
  switch (kind) {
    case "tuft":
      return <path d="M23 9.5c-.5-4 2.5-6 5-3.5M28.5 8.5c.8-4.2 4.6-4.6 6-1.8" {...line} />;
    case "glasses":
      return (
        <>
          <circle cx={LX} cy={EYE_Y} r="5.6" {...line} strokeWidth={1.8} />
          <circle cx={RX} cy={EYE_Y} r="5.6" {...line} strokeWidth={1.8} />
          <path d={`M${LX + 5.6} ${EYE_Y}h${RX - LX - 11.2}`} {...line} strokeWidth={1.8} />
        </>
      );
    case "ears":
      return <path d="M8.6 24.5c-4.6-.3-4.8 8 .2 8.2M47.4 24.5c4.6-.3 4.8 8-.2 8.2" {...line} />;
    case "none":
      return null;
  }
}

function Face({ mood }: { mood: Mood }) {
  const cx = (LX + RX) / 2;
  switch (mood) {
    case "waiting":
      return (
        <>
          <circle cx={LX} cy={EYE_Y} r="3.2" fill={INK} />
          <circle cx={RX} cy={EYE_Y} r="3.2" fill={INK} />
          <ellipse cx={cx} cy={MOUTH_Y + 1} rx="2.6" ry="3" {...line} />
        </>
      );
    case "working":
      return (
        <>
          <circle cx={LX + 1.2} cy={EYE_Y} r="2.1" fill={INK} />
          <circle cx={RX + 1.2} cy={EYE_Y} r="2.1" fill={INK} />
          <path d={`M${cx - 4} ${MOUTH_Y}q4 2.5 8 0`} {...line} />
        </>
      );
    case "ready":
      return (
        <>
          <path d={`M${LX - 3.5} ${EYE_Y + 1}q3.5-4.5 7 0M${RX - 3.5} ${EYE_Y + 1}q3.5-4.5 7 0`} {...line} />
          <path d={`M${cx - 6} ${MOUTH_Y - 1.5}q6 5.5 12 0`} {...line} />
        </>
      );
    case "done":
      return (
        <>
          <path d={`M${LX - 3.5} ${EYE_Y}h7M${RX - 3.5} ${EYE_Y}h7`} {...line} />
          <path d={`M${cx - 4} ${MOUTH_Y}h8`} {...line} />
        </>
      );
    case "error":
      return (
        <>
          <path
            d={`M${LX - 3} ${EYE_Y - 3}l6 6m0-6l-6 6M${RX - 3} ${EYE_Y - 3}l6 6m0-6l-6 6`}
            {...line}
          />
          <path d={`M${cx - 6} ${MOUTH_Y + 1}q1.5-2.5 3 0t3 0 3 0 3 0`} {...line} />
        </>
      );
  }
}

/** A scribbled "!" or "?" in the corner, in the accent colour. */
function Mark({ glyph }: { glyph: "!" | "?" }) {
  return (
    <g className="paper-mark">
      {glyph === "!" ? (
        <>
          <path d="M48 3.5l-1 9" />
          <circle cx="46.7" cy="16.5" r="1.4" />
        </>
      ) : (
        <>
          <path d="M43.5 6.5c.5-3.5 6.5-3.8 6.4-.2-.1 2.4-3 2.6-3.2 5.6" />
          <circle cx="46.7" cy="16" r="1.4" />
        </>
      )}
    </g>
  );
}

function mood(s: Session): Mood {
  if (needsUser(s)) return "waiting";
  if (s.status === "working") return "working";
  if (s.status === "error") return "error";
  if (s.status === "done") return "done";
  return "ready";
}

function Doodle({ session, size }: { session: Session; size: "sm" | "md" }) {
  const m = mood(session);
  const colour = COLOURS[session.avatar % COLOURS.length];
  const extra = EXTRAS[Math.floor(session.avatar / COLOURS.length + session.avatar) % EXTRAS.length];
  const mark = m === "waiting" ? (session.pending[0]?.kind === "question" ? "?" : "!") : null;
  return (
    <span className={`avatar avatar-${size} avatar-${m}`} aria-hidden="true">
      <svg viewBox="0 0 56 56" width="100%" height="100%">
        <g className="paper-doodle">
          <path d={HEAD} {...line} fill={colour} fillOpacity={m === "done" ? 0.45 : 1} />
          <Extras kind={extra} />
          <Face mood={m} />
        </g>
        {mark && size === "md" && <Mark glyph={mark} />}
      </svg>
    </span>
  );
}

export const paper: Theme = {
  id: "paper",
  name: "Paper",
  description: "Light and warm: ink on paper, with a hand-drawn doodle per session.",
  className: "theme-paper",
  defaultAccent: "#d9480f",
  accents: ["#d9480f", "#1c7ed6", "#2b8a3e", "#7048e8", "#c2255c"],

  Avatar: Doodle,

  Progress: ({ done, total }) => (
    <div
      className="paper-progress"
      role="progressbar"
      aria-valuemin={0}
      aria-valuemax={total}
      aria-valuenow={done}
      aria-label={`${done} of ${total} tasks done`}
    >
      <div className="paper-progress-fill" style={{ width: `${total ? (done / total) * 100 : 0}%` }} />
    </div>
  ),

  HelperIcon: ({ done }) => (
    <svg className="helper-icon" viewBox="0 0 16 16" width="14" height="14" aria-hidden="true">
      {done ? (
        <path d="M3 8.5l3.2 3L13 4.5" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" />
      ) : (
        <path d="M3 13l2-5 6-6 3 3-6 6z" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinejoin="round" />
      )}
    </svg>
  ),

  strings: {
    pillSummary: (c) => {
      const parts = [c.working && `${c.working} writing`, c.ready && `${c.ready} ready`, c.done && `${c.done} done`].filter(
        Boolean,
      ) as string[];
      if (c.needs) return { strong: `${c.needs} need${c.needs === 1 ? "s" : ""} you`, rest: parts.join(" · ") };
      if (parts.length) return { strong: parts[0], rest: parts.slice(1).join(" · ") };
      return { strong: "Agentry", rest: "desk's clear" };
    },
    emptyTitle: "Nothing on the desk",
    emptyHint: "Start claude in a terminal or your editor and it'll appear here.",
    waitingInTerminal: "Waiting for you in the terminal",
    allow: "Approve",
    deny: "Decline…",
    denyConfirm: "Decline",
    denyAndStop: "Decline & stop",
    notePlaceholder: "A note for Claude (optional)",
    send: "Send",
    skip: "Skip",
    useTerminal: "Use terminal",
    helpersHeading: "Helpers",
  },
};
