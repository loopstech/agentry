// The widget: a pill under the menu bar that expands into a panel.
//
// The native window stays the size of the open panel. Expanding and
// collapsing is one shape animating its size inside it (resizing the native
// window can't be kept in step with what the web view draws, which made the
// transition jump). The transparent rest of the window lets clicks through:
// the shape's rectangle is reported to the app, which ignores the mouse
// elsewhere.

import { useCallback, useEffect, useLayoutEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { accentStyle, themeFor } from "../themes";
import { countSessions, needsUser } from "../types";
import { plural } from "../format";
import { listen } from "@tauri-apps/api/event";
import {
  openSettings,
  respondRequest,
  setPanelOpen,
  useConfig,
  useLayout,
  useNow,
  useRequestArrivals,
  useSessions,
} from "../useSessions";
import { SessionRow } from "./SessionRow";
import { Alert, ChevronUp, Gear, Muted, Sound } from "./Icons";

/** Transparent margin around the content, so CSS shadows aren't clipped. */
const FRAME = 14;
/** On a notch: no margin at the top (the island is flush with the screen
 *  edge), room for the shadow and the notch "shoulders" elsewhere. */
const NOTCH_SIDE = 24;
const NOTCH_BOTTOM = 28;
/** Width of each "ear" either side of the notch when sessions exist. */
const NOTCH_EAR = 72;
const PANEL_WIDTH = 520;
const DRAG_THRESHOLD = 4;
const PILL_AVATARS = 4;
/** Hover-to-open: how long the pointer rests on the pill, and how long it can
 *  be away from the panel before it closes again. */
const HOVER_OPEN_MS = 350;
const HOVER_CLOSE_MS = 900;
/** The window is asked for this much height; the app caps it to the screen. */
const WINDOW_MAX_HEIGHT = 2000;

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
  const innerRef = useRef<HTMLDivElement>(null);
  const [shape, setShape] = useState<{ width: number; height: number } | null>(null);
  const [windowSize, setWindowSize] = useState<{
    width: number;
    height: number;
  } | null>(null);
  const counts = countSessions(sessions);
  const now = useNow(sessions.some(needsUser));
  const pulse = usePulse(counts.needs);
  const { config, setMuted } = useConfig();
  const muted = config?.muted ?? false;
  const theme = themeFor(config);
  const notch = useLayout().notch;
  const pad = notch ? { top: 0, x: NOTCH_SIDE, bottom: NOTCH_BOTTOM } : { top: FRAME, x: FRAME, bottom: FRAME };
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

  // Measure what's showing; the shape animates to that size (before the
  // first paint, so it starts at the right size without animating).
  useLayoutEffect(() => {
    const el = innerRef.current;
    if (!el) return;
    const measure = () =>
      setShape((prev) => {
        const next = { width: el.offsetWidth, height: el.offsetHeight };
        return prev && prev.width === next.width && prev.height === next.height ? prev : next;
      });
    measure();
    const ro = new ResizeObserver(measure);
    ro.observe(el);
    return () => ro.disconnect();
  }, [expanded]);

  // Size the window once for the widest thing it shows (normally the panel)
  // and as tall as the screen allows; the panel scrolls beyond that.
  const widest = Math.max(PANEL_WIDTH, shape?.width ?? 0);
  useEffect(() => {
    const want = { width: widest + pad.x * 2, height: WINDOW_MAX_HEIGHT };
    invoke<number>("set_panel_size", want)
      .then((applied) => {
        setWindowSize({ width: want.width, height: applied });
        setMaxPanelHeight(Math.max(120, applied - pad.top - pad.bottom));
      })
      .catch(() => {});
  }, [widest, pad.x, pad.top, pad.bottom]);

  // After a drag the app refits the window to the room below it.
  useEffect(() => {
    const sub = listen<{ width: number; height: number }>("agentry://window-size", (e) => {
      setWindowSize(e.payload);
      setMaxPanelHeight(Math.max(120, e.payload.height - pad.top - pad.bottom));
    });
    return () => {
      sub.then((off) => off());
    };
  }, [pad.top, pad.bottom]);

  // Only the shape takes the mouse; the rest of the window lets clicks through.
  useEffect(() => {
    if (!shape || !windowSize) return;
    invoke("set_hit_area", {
      x: (windowSize.width - shape.width) / 2,
      y: pad.top,
      width: shape.width,
      height: shape.height,
    }).catch(() => {});
  }, [shape, windowSize, pad.top]);

  // The global shortcut (default ⌥⌘A) opens and closes the panel.
  useEffect(() => {
    const sub = listen("agentry://toggle-panel", () => setExpanded((x) => !x));
    return () => {
      sub.then((off) => off());
    };
  }, []);

  // Esc closes; ⌘↵ allows and ⌘⌫ denies the top permission request.
  const topPermission = sessions.flatMap((s) => s.pending).find((p) => p.kind === "permission");
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") return setExpanded(false);
      if (!expanded || !topPermission || !(e.metaKey || e.ctrlKey)) return;
      if (document.activeElement instanceof HTMLInputElement) return;
      if (e.key === "Enter") {
        e.preventDefault();
        respondRequest(topPermission.id, { action: "allow" }).catch((err) => notify(String(err), true));
      } else if (e.key === "Backspace") {
        e.preventDefault();
        respondRequest(topPermission.id, { action: "deny" }).catch((err) => notify(String(err), true));
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [expanded, topPermission, notify]);

  const toggle = useCallback(() => setExpanded((x) => !x), []);
  const dragOrClick = useDragOrClick(toggle);
  // On a notch the island stays put: clicks only.
  const drag = notch ? { onClick: toggle } : dragOrClick;
  const summary = theme.strings.pillSummary(counts);

  return (
    <div
      className={`frame ${theme.className}${notch ? " frame-notch" : ""}`}
      style={{
        width: windowSize?.width,
        height: windowSize?.height,
        ...accentStyle(theme, config),
      }}
    >
      <div
        className={[
          "island-shape",
          notch ? "island-shape-notch" : "",
          expanded ? "is-expanded" : "",
          !expanded && pulse ? "pill-pulse" : "",
        ]
          .filter(Boolean)
          .join(" ")}
        style={
          {
            marginTop: pad.top,
            width: shape?.width,
            height: shape?.height,
            ...(notch ? { "--notch-h": `${notch.height}px` } : {}),
          } as React.CSSProperties
        }
        {...hover}
      >
        <div className="island-clip">
          <div ref={innerRef} key={expanded ? "panel" : "pill"} className="island-content">
            {!expanded && notch ? (
              <NotchPill
                notchWidth={notch.width}
                notchHeight={notch.height}
                sessions={sessions}
                counts={counts}
                theme={theme}
                pulse={pulse}
                label={`Agentry: ${summary.strong}${summary.rest ? ", " + summary.rest : ""}. Open panel`}
                onClick={toggle}
              />
            ) : !expanded ? (
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
                className={notch ? "panel panel-notch" : "panel"}
                aria-label="Agentry sessions"
                style={{
                  width: PANEL_WIDTH,
                  maxHeight: maxPanelHeight ?? undefined,
                }}
              >
                <header className="panel-header" {...drag} onClick={undefined}>
                  <span className="panel-title">Agentry</span>
                  <span className="panel-count">{plural(sessions.length, "session", "sessions")}</span>
                  <button
                    className="icon-button icon-button-filled"
                    aria-label="Settings"
                    title="Settings"
                    onClick={openSettings}
                  >
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
      </div>
    </div>
  );
}

/** The collapsed island on a notch: a black shape fused to the notch, with
 *  avatars on the left "ear" and the most important count on the right. With
 *  no sessions it's exactly the notch's size, so it disappears into it (and
 *  clicking the notch still opens the panel). */
function NotchPill(props: {
  notchWidth: number;
  notchHeight: number;
  sessions: ReturnType<typeof useSessions>["sessions"];
  counts: ReturnType<typeof countSessions>;
  theme: ReturnType<typeof themeFor>;
  pulse: boolean;
  label: string;
  onClick: () => void;
}) {
  const { sessions, counts, theme } = props;
  const ear = sessions.length > 0 ? NOTCH_EAR : 0;
  const [count, kind] = counts.needs
    ? [counts.needs, "needs"]
    : counts.working
      ? [counts.working, "working"]
      : [counts.ready + counts.done, "ready"];
  return (
    <button
      className={`notch-pill${props.pulse ? " pill-pulse" : ""}`}
      style={{ width: props.notchWidth + ear * 2, height: props.notchHeight }}
      aria-label={props.label}
      onClick={props.onClick}
    >
      {ear > 0 && (
        <>
          <span className="notch-ear notch-ear-left">
            <span className="pill-avatars">
              {sessions.slice(0, 3).map((s, i) => (
                <span key={s.id} className="pill-avatar" style={{ zIndex: 3 - i }}>
                  <theme.Avatar session={s} size="sm" />
                </span>
              ))}
            </span>
          </span>
          <span className="notch-ear notch-ear-right">
            <span className={`notch-count notch-count-${kind}`}>{count}</span>
          </span>
        </>
      )}
    </button>
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
      getCurrentWindow()
        .startDragging()
        .catch(() => {});
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
