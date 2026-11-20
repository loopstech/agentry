// Pixel: 8-bit. Every session is a little pixel-art sprite with an idle
// animation; chunky borders and a pixel font for headings.

import "@fontsource/silkscreen/400.css";
import "./pixel.css";
import type { Theme } from "../types";
import { needsUser, type Session } from "../../types";

type Mood = "working" | "waiting" | "ready" | "done" | "error";

const INK = "#1a1c2c";
const WHITE = "#f4f4f4";
const COLOURS = ["#38b764", "#41a6f6", "#ef7d57", "#ffcd75", "#73eff7", "#c78cff", "#a7f070", "#ff8fb1"];

interface Sprite {
  /** 12×12, "#" = body. */
  rows: string[];
  /** Left eye x, right eye x, eye y, mouth y (each eye is 2×2). */
  face: [number, number, number, number];
}

const SPRITES: Sprite[] = [
  {
    // slime
    rows: [
      "............",
      "............",
      "....####....",
      "..########..",
      ".##########.",
      ".##########.",
      "############",
      "############",
      "############",
      "############",
      ".#.##..##.#.",
      "............",
    ],
    face: [3, 7, 5, 8],
  },
  {
    // invader
    rows: [
      "............",
      "..#......#..",
      "...#....#...",
      "..########..",
      ".##########.",
      "############",
      "############",
      "############",
      "#.########.#",
      "#.#......#.#",
      "...##..##...",
      "............",
    ],
    face: [3, 7, 4, 7],
  },
  {
    // robot
    rows: [
      ".....##.....",
      ".....##.....",
      "..########..",
      "..########..",
      "############",
      "############",
      "..########..",
      "..########..",
      "..########..",
      "...######...",
      "...#....#...",
      "...#....#...",
    ],
    face: [3, 7, 3, 7],
  },
  {
    // cat
    rows: [
      "............",
      ".#........#.",
      ".##......##.",
      ".###....###.",
      ".##########.",
      "############",
      "############",
      "############",
      "############",
      ".##########.",
      "..#......#..",
      "............",
    ],
    face: [3, 7, 5, 8],
  },
];

/** The body as one rect per run of "#" in a row. */
function Body({ rows, colour }: { rows: string[]; colour: string }) {
  const rects: JSX.Element[] = [];
  rows.forEach((row, y) => {
    for (const m of row.matchAll(/#+/g)) {
      rects.push(<rect key={`${y}-${m.index}`} x={m.index} y={y} width={m[0].length} height={1} fill={colour} />);
    }
  });
  return <>{rects}</>;
}

const px = (x: number, y: number, fill: string, w = 1, h = 1) => (
  <rect key={`${x},${y},${fill}`} x={x} y={y} width={w} height={h} fill={fill} />
);

function Face({ face, mood }: { face: Sprite["face"]; mood: Mood }) {
  const [lx, rx, y, my] = face;
  const eyes = [lx, rx];
  switch (mood) {
    case "waiting":
      return (
        <>
          {eyes.map((x) => [px(x, y, WHITE, 2, 2), px(x + 1, y + 1, INK)])}
          {px(5, my, INK, 2, 2)}
        </>
      );
    case "working":
      return (
        <>
          {eyes.map((x) => [px(x, y, WHITE, 2, 2), px(x + 1, y, INK, 1, 2)])}
          {px(5, my, INK, 2)}
        </>
      );
    case "ready":
      return (
        <>
          {eyes.map((x) => [px(x, y + 1, INK), px(x + 1, y, INK)])}
          {[px(4, my, INK), px(5, my + 1, INK, 2), px(7, my, INK)]}
        </>
      );
    case "done":
      return (
        <>
          {eyes.map((x) => px(x, y + 1, INK, 2))}
          {px(5, my, INK, 2)}
        </>
      );
    case "error":
      return (
        <>
          {eyes.map((x) => [px(x, y, INK), px(x + 1, y + 1, INK)])}
          {[px(4, my + 1, INK), px(5, my, INK, 2), px(7, my + 1, INK)]}
        </>
      );
  }
}

function mood(s: Session): Mood {
  if (needsUser(s)) return "waiting";
  if (s.status === "working") return "working";
  if (s.status === "error") return "error";
  if (s.status === "done") return "done";
  return "ready";
}

function PixelSprite({ session, size }: { session: Session; size: "sm" | "md" }) {
  const sprite = SPRITES[session.avatar % SPRITES.length];
  const colour = COLOURS[(session.avatar * 3) % COLOURS.length];
  const m = mood(session);
  return (
    <span className={`avatar avatar-${size} avatar-${m}`} aria-hidden="true">
      <svg viewBox="0 0 12 12" width="100%" height="100%" shapeRendering="crispEdges">
        <g className="pixel-sprite">
          <Body rows={sprite.rows} colour={colour} />
          <Face face={sprite.face} mood={m} />
        </g>
      </svg>
    </span>
  );
}

const MAX_SEGMENTS = 12;

export const pixel: Theme = {
  id: "pixel",
  name: "Pixel",
  description: "8-bit: a pixel-art sprite per session, chunky borders, pixel font.",
  className: "theme-pixel",
  defaultAccent: "#ffcd75",
  accents: ["#ffcd75", "#38b764", "#41a6f6", "#ef7d57", "#ff8fb1"],

  Avatar: PixelSprite,

  // An HP-style bar of blocks.
  Progress: ({ done, total }) => {
    const n = Math.min(total, MAX_SEGMENTS);
    const filled = total <= MAX_SEGMENTS ? done : Math.round((done / total) * MAX_SEGMENTS);
    return (
      <div
        className="hp"
        role="progressbar"
        aria-valuemin={0}
        aria-valuemax={total}
        aria-valuenow={done}
        aria-label={`${done} of ${total} tasks done`}
      >
        {Array.from({ length: n }, (_, i) => (
          <span key={i} className={`hp-block${i < filled ? " hp-block-done" : ""}`} />
        ))}
      </div>
    );
  },

  HelperIcon: ({ done }) => (
    <svg className="helper-icon" viewBox="0 0 6 6" width="12" height="12" shapeRendering="crispEdges" aria-hidden="true">
      {done ? (
        <path d="M0 3h1v1H0zM1 4h1v1H1zM2 3h1v1H2zM3 2h1v1H3zM4 1h1v1H4z" fill="currentColor" />
      ) : (
        <path d="M1 0h1v6H1zM2 1h1v4H2zM3 2h1v2H3z" fill="currentColor" />
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
      return { strong: "Agentry", rest: "no players" };
    },
    emptyTitle: "No players yet",
    emptyHint: "Start claude in a terminal or your editor to join the party.",
    waitingInTerminal: "Waiting for you in the terminal",
    allow: "Yes",
    deny: "No…",
    denyConfirm: "No",
    denyAndStop: "No & stop",
    notePlaceholder: "Tell Claude why (optional)",
    send: "Send",
    skip: "Skip",
    useTerminal: "Use terminal",
    helpersHeading: "Party",
  },
};
