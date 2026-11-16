// The detail view: a session's recent steps, read from its transcript and
// refreshed while open.

import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Check, Close } from "./Icons";

interface Step {
  seq: number;
  kind: "prompt" | "text" | "tool" | "note";
  text: string;
  tool: string | null;
  status: "running" | "ok" | "error" | null;
  at: number | null;
}

const REFRESH_MS = 1500;

const last = <T,>(xs: T[]): T | undefined => xs[xs.length - 1];
const sameTail = (a: Step[], b: Step[]) =>
  a.length === b.length && last(a)?.seq === last(b)?.seq && last(a)?.status === last(b)?.status;

const time = (ms: number | null) =>
  ms ? new Date(ms).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" }) : "";

export function Transcript({ sessionId }: { sessionId: string }) {
  const [steps, setSteps] = useState<Step[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const listRef = useRef<HTMLOListElement>(null);
  const stickToBottom = useRef(true);

  useEffect(() => {
    let alive = true;
    const load = () =>
      invoke<Step[]>("get_transcript", { id: sessionId })
        .then((s) => {
          if (!alive) return;
          // Keep the same array when nothing changed, to avoid re-rendering.
          setSteps((prev) => (prev && sameTail(prev, s) ? prev : s));
          setError(null);
        })
        .catch((e) => alive && setError(String(e)));
    load();
    const t = setInterval(load, REFRESH_MS);
    return () => {
      alive = false;
      clearInterval(t);
    };
  }, [sessionId]);

  // Follow new steps unless the user has scrolled up.
  useLayoutEffect(() => {
    const el = listRef.current;
    if (el && stickToBottom.current) el.scrollTop = el.scrollHeight;
  }, [steps]);

  if (error && !steps) return <div className="transcript-empty">{error}</div>;
  if (!steps) return <div className="transcript-empty">Loading…</div>;
  if (steps.length === 0) return <div className="transcript-empty">Nothing in the transcript yet.</div>;

  return (
    <ol
      ref={listRef}
      className="transcript"
      aria-label="Recent steps"
      onScroll={(e) => {
        const el = e.currentTarget;
        stickToBottom.current = el.scrollHeight - el.scrollTop - el.clientHeight < 24;
      }}
    >
      {steps.map((s) => (
        <li key={s.seq} className={`step step-${s.kind}`}>
          {s.kind === "tool" && (
            <span className={`step-status step-status-${s.status ?? "running"}`} aria-label={s.status ?? "running"}>
              {s.status === "ok" ? <Check /> : s.status === "error" ? <Close /> : null}
            </span>
          )}
          {s.kind === "prompt" && <span className="step-who">You</span>}
          <span className="step-text">{s.text}</span>
          {s.kind === "prompt" && <span className="step-time">{time(s.at)}</span>}
        </li>
      ))}
    </ol>
  );
}
