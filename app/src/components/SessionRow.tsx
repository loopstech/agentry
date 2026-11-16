import type { Theme } from "../themes/types";
import { needsUser, type Session } from "../types";
import { elapsed } from "../format";
import { Check, Close } from "./Icons";
import { dismissSession } from "../useSessions";

interface Props {
  session: Session;
  theme: Theme;
  now: number;
}

export function SessionRow({ session: s, theme, now }: Props) {
  const { Avatar, Progress } = theme;
  const needs = needsUser(s);
  const helpers = s.helpers;

  return (
    <li className={`row row-${s.status}${needs ? " row-needs" : ""}`}>
      <div className="row-main">
        <Avatar session={s} size="md" />
        <div className="row-text">
          <div className="row-title">
            <span className="title">{s.title}</span>
            <span className="project"> — {s.project}</span>
          </div>
          <div className={`row-subtitle${needs ? " is-needs" : s.status === "error" ? " is-error" : ""}`}>
            {s.subtitle}
          </div>
          {needs && s.needsSince && (
            <div className="row-hint">
              {theme.strings.waitingInTerminal} · {elapsed(now - s.needsSince)}
            </div>
          )}
        </div>
        {s.progress && (
          <span className="row-count" aria-hidden="true">
            {s.progress.done}/{s.progress.total}
          </span>
        )}
        <button
          className="icon-button"
          aria-label={`Dismiss ${s.title}`}
          title="Dismiss"
          onClick={() => dismissSession(s.id)}
        >
          <Close />
        </button>
      </div>

      {s.progress && s.progress.total > 0 && (
        <div className="row-indent">
          <Progress done={s.progress.done} total={s.progress.total} />
        </div>
      )}

      {helpers.length > 0 && (
        <ul className="row-indent helpers" aria-label="Helpers">
          {helpers.map((h) => (
            <li key={h.agentId} className={`helper${h.done ? " helper-done" : ""}`}>
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
