// The widget: a pill under the menu bar that expands into a panel.
// The native window is resized to fit whatever is showing.

import { useCallback, useEffect, useLayoutEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { accentStyle, themeFor } from "../themes";
import { countSessions, needsUser } from "../types";
import { plural } from "../format";
import { openSettings, setPanelOpen, useConfig, useNow, useRequestArrivals, useSessions } from "../useSessions";
import { SessionRow } from "./SessionRow";
import { Alert, ChevronUp, Gear, Muted, Sound } from "./Icons";

/** Transparent margin around the content, so CSS shadows aren't clipped. */
const FRAME = 14;
const PANEL_WIDTH = 520;
const DRAG_THRESHOLD = 4;
const PILL_AVATARS = 4;
/** Hover-to-open: how long the pointer rests on the pill, and how long it can
 *  be away from the panel before it closes again. */
const HOVER_OPEN_MS = 350;
const HOVER_CLOSE_MS = 900;

export function Island() {
  const { sessions, server } = useSessions();
  const [expanded, setExpanded] = useState(false);
  const [maxPanelHeight, setMaxPanelHeight] = useState<number | null>(null);
  const [openRow, setOpenRow] = useState<string | null>(null);
  const [toast, setToast] = useState<{ text: string; error: boolean } | null>(null);
  const notify = useCallback((text: string, error = false) => setToast({ text, error }), []);
  useEffect(() => {
    if (!toast) return;
    const t = setTimeout(() => setToast(null), toast.error ? 6000 : 2500);
    return () => clearTimeout(t);
  }, [toast]);
  const contentRef = useRef<HTMLDivElement>(null);
  const counts = countSessions(sessions);
  const now = useNow(sessions.some(needsUser));
  const pulse = usePulse(counts.needs);
  const { config, setMuted } = useConfig();
  const muted = config?.muted ?? false;
  const theme = themeFor(config);
  const hover = useHoverToOpen(
    config?.appearance.hoverToOpen ?? false,
    expanded,
    setExpanded,
    sessions.some((s) => s.pending.length > 0),
  );

  // Open the panel when a request arrives (the user can turn this off).
  useRequestArrivals(
    useCallback((r: { openPanel: boolean }) => {
      if (r.openPanel) setExpanded(true);
    }, []),
  );

  // The server uses this for "answer in the widget only while it's open".
  useEffect(() => {
    setPanelOpen(expanded);
  }, [expanded]);

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
    <div className={`frame ${theme.className}`} style={{ padding: FRAME, ...accentStyle(theme, config) }}>
      <div ref={contentRef} className="content" {...hover}>
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
            {muted && (
              <span className="pill-muted" title="Quiet mode: prompts go to the terminal">
                <Muted />
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
              <button className="icon-button icon-button-filled" aria-label="Settings" title="Settings" onClick={openSettings}>
                <Gear />
              </button>
              <button
                className={`icon-button icon-button-filled${muted ? " is-on" : ""}`}
                aria-pressed={muted}
                aria-label={muted ? "Quiet mode is on: prompts go to the terminal. Turn off" : "Turn on quiet mode"}
                title={muted ? "Quiet mode on — prompts go to the terminal" : "Quiet mode"}
                onClick={() => setMuted(!muted)}
              >
                {muted ? <Muted /> : <Sound />}
              </button>
              <button
                className="icon-button icon-button-filled"
                aria-label="Collapse to pill"
                title="Collapse"
                onClick={() => setExpanded(false)}
              >
                <ChevronUp />
              </button>
            </header>

            {toast && (
              <div className={`toast${toast.error ? " toast-error" : ""}`} role="status">
                {toast.text}
              </div>
            )}

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
                  <SessionRow
                    key={s.id}
                    session={s}
                    theme={theme}
                    now={now}
                    open={openRow === s.id}
                    onToggle={() => setOpenRow((cur) => (cur === s.id ? null : s.id))}
                    notify={notify}
                  />
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

/** Pointer handlers for "open the panel on hover": opens after resting on
 *  the pill, closes after leaving the panel, unless a request is waiting or
 *  the user is typing. */
function useHoverToOpen(enabled: boolean, expanded: boolean, setExpanded: (v: boolean) => void, busy: boolean) {
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const openedByHover = useRef(false);
  const clear = () => {
    if (timer.current) clearTimeout(timer.current);
    timer.current = null;
  };
  useEffect(() => clear, []);
  if (!enabled) return {};
  return {
    onMouseEnter: () => {
      clear();
      if (!expanded) {
        timer.current = setTimeout(() => {
          openedByHover.current = true;
          setExpanded(true);
        }, HOVER_OPEN_MS);
      }
    },
    onMouseLeave: () => {
      clear();
      if (expanded && openedByHover.current && !busy) {
        timer.current = setTimeout(() => {
          const typing = document.activeElement instanceof HTMLInputElement;
          if (!typing) {
            openedByHover.current = false;
            setExpanded(false);
          }
        }, HOVER_CLOSE_MS);
      }
    },
  };
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
