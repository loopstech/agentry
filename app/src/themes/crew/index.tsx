// Crew: deep navy, chunky type, and each session is a little character
// (design direction E).

import "./crew.css";
import type { Theme } from "../types";
import { needsUser, type Session } from "../../types";

type Shape = "robot" | "blob" | "ghost" | "triangle";
type Mood = "working" | "waiting" | "ready" | "done" | "error";

const SHAPES: Shape[] = ["robot", "blob", "ghost", "triangle"];
const COLOURS = [
  ["#2ec4b6", "#ff8a65", "#b79cff", "#ff6fa8"],
  ["#7fe0d6", "#ffb38a", "#9db2ff", "#ff9ec4"],
];

/** Eye centres per shape (56×56 box). */
const EYES: Record<Shape, [number, number, number]> = {
  robot: [20, 36, 30],
  blob: [21, 35, 30],
  ghost: [21, 35, 30],
  triangle: [23, 33, 35],
};

const INK = "#0f1030";

function Body({ shape, colour }: { shape: Shape; colour: string }) {
  switch (shape) {
    case "robot":
      return (
        <>
          <line x1="28" y1="3" x2="28" y2="11" stroke={colour} strokeWidth="3" strokeLinecap="round" />
          <circle cx="28" cy="4" r="3.5" className="crew-antenna" />
          <rect x="6" y="11" width="44" height="41" rx="14" fill={colour} />
        </>
      );
    case "blob":
      return <circle cx="28" cy="31" r="22" fill={colour} />;
    case "ghost":
      return <path d="M8 52V30a20 20 0 0 1 40 0v22l-6.6-5-6.7 5-6.7-5-6.7 5-6.6-5z" fill={colour} />;
    case "triangle":
      return <path d="M28 9l21 38H7z" fill={colour} stroke={colour} strokeWidth="6" strokeLinejoin="round" />;
  }
}

function Face({ shape, mood }: { shape: Shape; mood: Mood }) {
  const [lx, rx, y] = EYES[shape];
  const k = shape === "triangle" ? 0.8 : 1;
  const mouthY = y + 12 * k;
  const cx = (lx + rx) / 2;
  switch (mood) {
    case "waiting":
      return (
        <>
          {[lx, rx].map((x) => (
            <g key={x}>
              <circle cx={x} cy={y} r={7 * k} fill="#fff" />
              <circle cx={x} cy={y} r={3.5 * k} fill={INK} />
            </g>
          ))}
          <ellipse cx={cx} cy={mouthY + 1} rx={3 * k} ry={3.5 * k} fill={INK} />
        </>
      );
    case "working":
      return (
        <>
          {[lx, rx].map((x) => (
            <g key={x}>
              <circle cx={x} cy={y} r={5.5 * k} fill="#fff" />
              <circle cx={x + 1.5} cy={y} r={2.8 * k} fill={INK} className="crew-pupil" />
            </g>
          ))}
          <path d={`M${cx - 6 * k} ${mouthY - 1}q${6 * k} ${5 * k} ${12 * k} 0`} fill="none" stroke={INK} strokeWidth="2.5" strokeLinecap="round" />
        </>
      );
    case "ready":
      return (
        <>
          {[lx, rx].map((x) => (
            <path key={x} d={`M${x - 4 * k} ${y + 1}q${4 * k} -${5 * k} ${8 * k} 0`} fill="none" stroke={INK} strokeWidth="2.5" strokeLinecap="round" />
          ))}
          <path d={`M${cx - 6 * k} ${mouthY - 2}q${6 * k} ${5 * k} ${12 * k} 0`} fill="none" stroke={INK} strokeWidth="2.5" strokeLinecap="round" />
        </>
      );
    case "done":
      return (
        <>
          {[lx, rx].map((x) => (
            <path key={x} d={`M${x - 4 * k} ${y}h${8 * k}`} stroke={INK} strokeWidth="2.5" strokeLinecap="round" />
          ))}
          <path d={`M${cx - 4 * k} ${mouthY - 1}h${8 * k}`} stroke={INK} strokeWidth="2.5" strokeLinecap="round" />
        </>
      );
    case "error":
      return (
        <>
          {[lx, rx].map((x) => (
            <path key={x} d={`M${x - 3.5 * k} ${y - 3.5 * k}l${7 * k} ${7 * k}M${x + 3.5 * k} ${y - 3.5 * k}l-${7 * k} ${7 * k}`} stroke={INK} strokeWidth="2.5" strokeLinecap="round" />
          ))}
          <path d={`M${cx - 6 * k} ${mouthY + 2}q${6 * k} -${5 * k} ${12 * k} 0`} fill="none" stroke={INK} strokeWidth="2.5" strokeLinecap="round" />
        </>
      );
  }
}

function Badge({ glyph }: { glyph: "!" | "?" }) {
  return (
    <g className="crew-badge">
      <circle cx="47" cy="10" r="9" />
      {glyph === "!" ? (
        <>
          <rect x="45.6" y="4.5" width="2.8" height="7" rx="1.4" fill={INK} />
          <circle cx="47" cy="14.6" r="1.6" fill={INK} />
        </>
      ) : (
        <>
          <path d="M44 7.5a3 3 0 1 1 3.8 2.9c-.8.3-.8.9-.8 1.6" fill="none" stroke={INK} strokeWidth="2.2" strokeLinecap="round" />
          <circle cx="47" cy="15" r="1.4" fill={INK} />
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

export function Creature({ session, size }: { session: Session; size: "sm" | "md" }) {
  const shape = SHAPES[session.avatar % SHAPES.length];
  const colour = COLOURS[Math.floor(session.avatar / SHAPES.length) % COLOURS.length][session.avatar % SHAPES.length];
  const m = mood(session);
  const badge = m === "waiting" ? (session.pending[0]?.kind === "question" ? "?" : "!") : null;
  return (
    <span className={`avatar avatar-${size} avatar-${m}`} aria-hidden="true">
      <svg viewBox="0 0 56 56" width="100%" height="100%">
        <g className="crew-body">
          <Body shape={shape} colour={colour} />
          <Face shape={shape} mood={m} />
        </g>
        {badge && size === "md" && <Badge glyph={badge} />}
      </svg>
    </span>
  );
}

const MAX_DOTS = 12;

export const crew: Theme = {
  id: "crew",
  name: "Crew",
  description: "Deep navy, chunky type, and a little character for every session.",
  className: "theme-crew",
  defaultAccent: "#ffd166",
  accents: ["#ffd166", "#7fe0d6", "#ff8a65", "#b79cff", "#ff6fa8"],

  Avatar: Creature,

  Progress: ({ done, total }) => {
    // One dot per task, or a scaled row of dots for long lists.
    const dots = Math.min(total, MAX_DOTS);
    const filled = total <= MAX_DOTS ? done : Math.round((done / total) * MAX_DOTS);
    return (
      <div className="dots" role="progressbar" aria-valuemin={0} aria-valuemax={total} aria-valuenow={done} aria-label={`${done} of ${total} tasks done`}>
        {Array.from({ length: dots }, (_, i) => (
          <span key={i} className={`dot${i < filled ? " dot-done" : i === filled ? " dot-current" : ""}`} />
        ))}
      </div>
    );
  },

  HelperIcon: ({ done }) => (
    <svg className="helper-icon" viewBox="0 0 20 20" width="18" height="18" aria-hidden="true">
      <rect x="1" y="1" width="18" height="18" rx="6" fill={done ? "#5a5c9a" : "#7fe0d6"} />
      {done ? (
        <path d="M5 9h3M12 9h3" stroke={INK} strokeWidth="1.8" strokeLinecap="round" />
      ) : (
        <>
          <circle cx="7" cy="9" r="1.8" fill={INK} />
          <circle cx="13" cy="9" r="1.8" fill={INK} />
        </>
      )}
    </svg>
  ),

  strings: {
    pillSummary: (c) => {
      const parts = [c.working && `${c.working} busy`, c.ready && `${c.ready} ready`, c.done && `${c.done} done`].filter(
        Boolean,
      ) as string[];
      if (c.needs) return { strong: `${c.needs} need${c.needs === 1 ? "s" : ""} you`, rest: parts.join(" · ") };
      if (parts.length) return { strong: parts[0], rest: parts.slice(1).join(" · ") };
      return { strong: "Agentry", rest: "crew's empty" };
    },
    emptyTitle: "Your crew is empty",
    emptyHint: "Start claude in a terminal or your editor and they'll turn up here.",
    waitingInTerminal: "Waiting for you in the terminal",
    allow: "Go for it",
    deny: "Nope…",
    denyConfirm: "Nope",
    denyAndStop: "Nope, and stop",
    notePlaceholder: "Tell Claude why (optional)",
    send: "Send",
    skip: "Skip",
    useTerminal: "Use terminal",
    helpersHeading: "Helpers",
  },
};
