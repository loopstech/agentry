import type { Theme } from "../themes/types";
import { needsUser, type Session } from "../types";
import { elapsed } from "../format";
import { Chat, Check, Close, Jump } from "./Icons";
import { dismissSession, jumpToSession, setTalk } from "../useSessions";
import { MessageBox, SpeechBubble } from "./Chat";
import { RequestCard } from "./RequestCard";
import { Transcript } from "./Transcript";

interface Props {
  session: Session;
  theme: Theme;
  now: number;
  open: boolean;
  onToggle: () => void;
  /** Show a short message (e.g. the result of jumping). */
  notify: (message: string, isError?: boolean) => void;
}

export function SessionRow({ session: s, theme, now, open, onToggle, notify }: Props) {
  const { Avatar, Progress } = theme;
  const needs = needsUser(s);
  const helpers = s.helpers;
  const request = s.pending[0];
  const jump = () =>
    jumpToSession(s.id).then(
      (msg) => notify(msg),
      (err) => notify(String(err), true),
    );

  return (
    <li className={`row row-${s.status}${needs ? " row-needs" : ""}`}>
      <div className="row-main">
        <Avatar session={s} size="md" />
        <button
          className="row-text"
          aria-expanded={open}
          aria-label={`${s.title}: ${s.subtitle}. ${open ? "Hide" : "Show"} recent steps`}
          onClick={onToggle}
        >
          <span className="row-title">
            <span className="title">{s.title}</span>
            <span className="project"> — {s.project}</span>
          </span>
          <span className={`row-subtitle${needs ? " is-needs" : s.status === "error" ? " is-error" : ""}`}>
            {s.subtitle}
          </span>
          {needs && !request && s.needsSince && (
            <span className="row-hint">
              {theme.strings.waitingInTerminal} · {elapsed(now - s.needsSince)}
            </span>
          )}
        </button>
        {s.progress && (
          <span className="row-count" aria-hidden="true">
            {s.progress.done}/{s.progress.total}
          </span>
        )}
        <button
          className={`icon-button${s.talk ? " icon-button-on" : ""}`}
          aria-pressed={s.talk}
          aria-label={`Chat with ${s.title}`}
          title={s.talk ? "Hide chat" : "Chat"}
          onClick={() => setTalk(s.id, !s.talk).catch(() => {})}
        >
          <Chat />
        </button>
        <button className="icon-button" aria-label={`Go to ${s.title}`} title="Go to session" onClick={jump}>
          <Jump />
        </button>
        <button
          className="icon-button"
          aria-label={`Dismiss ${s.title}`}
          title="Dismiss"
          onClick={() => dismissSession(s.id)}
        >
          <Close />
        </button>
      </div>

      {request && (
        <div className="row-indent">
          <RequestCard key={request.id} request={request} now={now} theme={theme} onUseTerminal={jump} />
          {s.pending.length > 1 && <div className="request-more">+{s.pending.length - 1} more waiting</div>}
        </div>
      )}

      {s.progress && s.progress.total > 0 && (
        <div className="row-indent">
          <Progress done={s.progress.done} total={s.progress.total} />
        </div>
      )}

      {s.talk && (
        <div className="row-indent chat">
          <SpeechBubble session={s} />
          <MessageBox session={s} notify={notify} />
        </div>
      )}

      {open && (
        <div className="row-indent">
          <Transcript sessionId={s.id} />
        </div>
      )}

      {helpers.length > 0 && (
        <ul className="row-indent helpers" aria-label={theme.strings.helpersHeading ?? "Helpers"}>
          {helpers.map((h) => (
            <li key={h.agentId} className={`helper${h.done ? " helper-done" : ""}`}>
              {theme.HelperIcon && <theme.HelperIcon done={h.done} />}
              <span className="helper-type">{h.agentType}</span>
              <span className="helper-subtitle">{h.subtitle}</span>
              {h.done && <Check />}
            </li>
          ))}
        </ul>
      )}
    </li>
  );
}
