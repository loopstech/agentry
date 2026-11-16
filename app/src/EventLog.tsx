// Phase 1 debug view: the server's status and a live list of hook events.
// Replaced by the widget in Phase 2.

import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

interface ServerInfo {
  port: number | null;
  error: string | null;
}

interface EventSummary {
  event: string;
  session_id: string | null;
  cwd: string | null;
  tool_name: string | null;
  agent_id: string | null;
  agent_type: string | null;
  notification_type: string | null;
  blocking: boolean;
  received_at_ms: number;
}

const MAX_EVENTS = 300;

const colors = {
  bg: "#000000",
  row: "#0d0d0d",
  line: "#1f1f1f",
  text: "#f5f5f5",
  muted: "#a1a1a1",
  accent: "#ff7a45",
};

export function EventLog() {
  const [info, setInfo] = useState<ServerInfo>({ port: null, error: null });
  const [events, setEvents] = useState<EventSummary[]>([]);

  useEffect(() => {
    invoke<ServerInfo>("server_info").then(setInfo).catch(() => {});
    const unServer = listen<ServerInfo>("agentry://server", (e) => setInfo(e.payload));
    const unEvent = listen<EventSummary>("agentry://event", (e) =>
      setEvents((prev) => [e.payload, ...prev].slice(0, MAX_EVENTS)),
    );
    return () => {
      unServer.then((f) => f());
      unEvent.then((f) => f());
    };
  }, []);

  const sessions = new Set(events.map((e) => e.session_id).filter(Boolean)).size;

  return (
    <main
      style={{
        minHeight: "100vh",
        background: colors.bg,
        color: colors.text,
        fontFamily: "-apple-system, system-ui, sans-serif",
        fontSize: 13,
      }}
    >
      <header
        style={{
          position: "sticky",
          top: 0,
          background: colors.bg,
          borderBottom: `1px solid ${colors.line}`,
          padding: "14px 16px",
          display: "flex",
          gap: 12,
          alignItems: "baseline",
          flexWrap: "wrap",
        }}
      >
        <strong style={{ fontSize: 15 }}>Agentry</strong>
        <span style={{ color: info.error ? colors.accent : colors.muted }}>
          {info.error
            ? `Server error: ${info.error}`
            : info.port
              ? `Listening on 127.0.0.1:${info.port}`
              : "Starting…"}
        </span>
        <span style={{ flexGrow: 1 }} />
        <span style={{ color: colors.muted }}>
          {events.length} events · {sessions} sessions
        </span>
        <button
          onClick={() => setEvents([])}
          style={{
            background: "#1f1f1f",
            color: colors.text,
            border: 0,
            borderRadius: 12,
            padding: "4px 12px",
            font: "inherit",
          }}
        >
          Clear
        </button>
      </header>

      {events.length === 0 ? (
        <p style={{ color: colors.muted, padding: 16, lineHeight: 1.5 }}>
          No events yet. Run <code>agentry hooks install</code>, then start a Claude Code session.
        </p>
      ) : (
        <ol style={{ listStyle: "none", margin: 0, padding: 0 }}>
          {events.map((e, i) => (
            <li
              key={`${e.received_at_ms}-${i}`}
              style={{
                display: "grid",
                gridTemplateColumns: "72px 150px minmax(0, 1fr)",
                gap: 10,
                padding: "6px 16px",
                borderBottom: `1px solid ${colors.line}`,
                fontFamily: "ui-monospace, Menlo, monospace",
                fontSize: 12,
              }}
            >
              <span style={{ color: colors.muted }}>
                {new Date(e.received_at_ms).toLocaleTimeString()}
              </span>
              <span style={{ color: e.blocking ? colors.accent : colors.text }}>{e.event}</span>
              <span style={{ color: colors.muted, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
                {[
                  e.session_id?.slice(0, 8),
                  e.cwd?.split("/").pop(),
                  e.tool_name,
                  e.agent_type && `agent:${e.agent_type}`,
                  e.notification_type,
                ]
                  .filter(Boolean)
                  .join(" · ")}
              </span>
            </li>
          ))}
        </ol>
      )}
    </main>
  );
}
