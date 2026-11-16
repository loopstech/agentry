// The widget: a pill under the menu bar that expands into a panel.
// The native window is resized to fit whatever is showing.

import { useCallback, useEffect, useLayoutEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import type { Theme } from "../themes/types";
import { countSessions, needsUser } from "../types";
import { plural } from "../format";
import { useNow, useSessions } from "../useSessions";
import { SessionRow } from "./SessionRow";
import { Alert, ChevronUp } from "./Icons";

/** Transparent margin around the content, so CSS shadows aren't clipped. */
const FRAME = 14;
const PANEL_WIDTH = 520;
const DRAG_THRESHOLD = 4;
const PILL_AVATARS = 4;

export function Island({ theme }: { theme: Theme }) {
  const { sessions, server } = useSessions();
  const [expanded, setExpanded] = useState(false);
  const [maxPanelHeight, setMaxPanelHeight] = useState<number | null>(null);
  const contentRef = useRef<HTMLDivElement>(null);
  const counts = countSessions(sessions);
  const now = useNow(sessions.some(needsUser));
  const pulse = usePulse(counts.needs);

  // Fit the window to the content.
  useLayoutEffect(() => {
    const el = contentRef.current;
    if (!el) return;
    const fit = () => {
      const want = { width: el.offsetWidth + FRAME * 2, height: el.scrollHeight + FRAME * 2 };
      invoke<number>("set_panel_size", want)
        .then((applied) => setMaxPanelHeight(applied < want.height ? applied - FRAME * 2 : null))
        .catch(() => {});
    };
    fit();
    const ro = new ResizeObserver(fit);
    ro.observe(el);
    return () => ro.disconnect();
  }, [expanded]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && setExpanded(false);
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  const drag = useDragOrClick(() => setExpanded((x) => !x));
  const summary = theme.strings.pillSummary(counts);

  return (
    <div className={`frame ${theme.className}`} style={{ padding: FRAME }}>
      <div ref={contentRef} className="content">
        {!expanded ? (
          <button
            className={`pill${counts.needs ? " pill-needs" : ""}${pulse ? " pill-pulse" : ""}`}
            aria-expanded={false}
            aria-label={`Agentry: ${summary.strong}${summary.rest ? ", " + summary.rest : ""}. Open panel`}
            {...drag}
          >
            {sessions.length > 0 && (
              <span className="pill-avatars">
                {sessions.slice(0, PILL_AVATARS).map((s, i) => (
                  // Earlier (more urgent) avatars sit on top of later ones.
                  <span key={s.id} className="pill-avatar" style={{ zIndex: PILL_AVATARS - i }}>
                    <theme.Avatar session={s} size="sm" />
                  </span>
                ))}
                {sessions.length > PILL_AVATARS && (
                  <span className="pill-more">+{sessions.length - PILL_AVATARS}</span>
                )}
              </span>
            )}
            <span className="pill-strong">{summary.strong}</span>
            {summary.rest && <span className="pill-rest">· {summary.rest}</span>}
            {counts.needs > 0 && (
              <span className="pill-alert">
                <Alert />
              </span>
            )}
          </button>
        ) : (
          <section
            className="panel"
            aria-label="Agentry sessions"
            style={{ width: PANEL_WIDTH, maxHeight: maxPanelHeight ?? undefined }}
          >
            <header className="panel-header" {...drag} onClick={undefined}>
              <span className="panel-title">Agentry</span>
              <span className="panel-count">{plural(sessions.length, "session", "sessions")}</span>
              <button
                className="icon-button icon-button-filled"
                aria-label="Collapse to pill"
                title="Collapse"
                onClick={() => setExpanded(false)}
              >
                <ChevronUp />
              </button>
            </header>

            {server.error ? (
              <div className="empty">
                <div className="empty-title">Agentry couldn’t start</div>
                <div className="empty-hint">{server.error}</div>
              </div>
            ) : sessions.length === 0 ? (
              <div className="empty">
                <div className="empty-title">{theme.strings.emptyTitle}</div>
                <div className="empty-hint">{theme.strings.emptyHint}</div>
              </div>
            ) : (
              <ul className="rows">
                {sessions.map((s) => (
                  <SessionRow key={s.id} session={s} theme={theme} now={now} />
                ))}
              </ul>
            )}
          </section>
        )}
      </div>
    </div>
  );
}

/** Pointer handlers that drag the window once the pointer moves a few pixels,
 *  and otherwise treat the press as a click. */
function useDragOrClick(onClick: () => void) {
  const start = useRef<{ x: number; y: number } | null>(null);
  const dragged = useRef(false);

  const onMouseDown = useCallback((e: React.MouseEvent) => {
    if (e.button !== 0) return;
    if ((e.target as HTMLElement).closest(".icon-button")) return;
    start.current = { x: e.screenX, y: e.screenY };
    dragged.current = false;
  }, []);

  const onMouseMove = useCallback((e: React.MouseEvent) => {
    const s = start.current;
    if (!s || dragged.current) return;
    if (Math.abs(e.screenX - s.x) + Math.abs(e.screenY - s.y) > DRAG_THRESHOLD) {
      dragged.current = true;
      start.current = null;
      getCurrentWindow().startDragging().catch(() => {});
    }
  }, []);

  const onMouseUp = useCallback(() => {
    start.current = null;
  }, []);

  const onClickHandler = useCallback(() => {
    if (dragged.current) {
      dragged.current = false;
      return;
    }
    onClick();
  }, [onClick]);

  return { onMouseDown, onMouseMove, onMouseUp, onClick: onClickHandler };
}

/** True for a moment whenever the number of sessions needing you goes up. */
function usePulse(needs: number) {
  const prev = useRef(needs);
  const [pulse, setPulse] = useState(false);
  useEffect(() => {
    if (needs > prev.current) {
      setPulse(true);
      const t = setTimeout(() => setPulse(false), 1600);
      prev.current = needs;
      return () => clearTimeout(t);
    }
    prev.current = needs;
  }, [needs]);
  return pulse;
}
