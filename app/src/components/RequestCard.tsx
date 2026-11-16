// Inline controls for a request waiting in the widget: Allow / Deny / Always
// allow / Terminal for permissions, option chips for questions.

import { useState } from "react";
import type { Pending, Question, UiResponse } from "../types";
import type { Theme } from "../themes/types";
import { elapsed } from "../format";
import { respondRequest } from "../useSessions";

interface Props {
  request: Pending;
  now: number;
  theme: Theme;
  /** Bring the session's terminal forward (after handing the request back). */
  onUseTerminal: () => void;
}

export function RequestCard({ request, now, theme, onUseTerminal }: Props) {
  const t = theme.strings;
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const send = async (response: UiResponse) => {
    setBusy(true);
    setError(null);
    try {
      await respondRequest(request.id, response);
      if (response.action === "terminal") onUseTerminal();
    } catch (e) {
      setError(String(e));
      setBusy(false);
    }
  };

  const remaining = request.expiresAt ? request.expiresAt - now : null;

  return (
    <div className="request" aria-busy={busy}>
      {request.agentType && <div className="request-agent">Asked by the {request.agentType} helper</div>}
      {request.kind === "permission" ? (
        <PermissionBody request={request} send={send} busy={busy} t={t} />
      ) : (
        <QuestionBody request={request} send={send} busy={busy} t={t} />
      )}
      <div className="request-footer">
        <span>
          You can also answer in the terminal
          {remaining !== null && remaining > 0 && <> · widget hands over in {elapsed(remaining)}</>}
        </span>
      </div>
      {error && (
        <div className="request-error" role="alert">
          {error}
        </div>
      )}
    </div>
  );
}

type Send = (r: UiResponse) => void;
type Strings = Theme["strings"];

function PermissionBody({ request, send, busy, t }: { request: Pending; send: Send; busy: boolean; t: Strings }) {
  const [denying, setDenying] = useState(false);
  const [note, setNote] = useState("");

  return (
    <>
      {request.detail && (
        <figure className="code">
          {request.detailLabel && <figcaption className="code-label">{request.detailLabel}</figcaption>}
          <pre>{request.detail}</pre>
        </figure>
      )}

      {!denying ? (
        <>
          <div className="actions">
            <button className="btn btn-primary" disabled={busy} onClick={() => send({ action: "allow" })}>
              {t.allow}
              <span className="kbd" aria-hidden="true">⌘↵</span>
            </button>
            <button className="btn" disabled={busy} onClick={() => setDenying(true)}>
              {t.deny}
            </button>
            <span className="actions-spacer" />
            <button className="btn btn-link" disabled={busy} onClick={() => send({ action: "terminal" })}>
              {t.useTerminal}
            </button>
          </div>
          {request.alwaysAllowLabel && (
            <button
              className="btn btn-ghost btn-block"
              disabled={busy}
              onClick={() => send({ action: "always_allow" })}
              title={request.alwaysAllowLabel}
            >
              {request.alwaysAllowLabel}
            </button>
          )}
        </>
      ) : (
        <form
          className="deny"
          onSubmit={(e) => {
            e.preventDefault();
            send({ action: "deny", note: note.trim() || undefined });
          }}
        >
          <label className="field">
            <span className="field-label">Note to Claude (optional)</span>
            <input
              autoFocus
              type="text"
              value={note}
              placeholder={t.notePlaceholder}
              onChange={(e) => setNote(e.target.value)}
            />
          </label>
          <div className="actions">
            <button type="submit" className="btn btn-primary" disabled={busy}>
              {t.denyConfirm}
            </button>
            <button
              type="button"
              className="btn"
              disabled={busy}
              onClick={() => send({ action: "deny", note: note.trim() || undefined, stop: true })}
            >
              {t.denyAndStop}
            </button>
            <span className="actions-spacer" />
            <button type="button" className="btn btn-link" onClick={() => setDenying(false)}>
              Back
            </button>
          </div>
        </form>
      )}
    </>
  );
}

function QuestionBody({ request, send, busy, t }: { request: Pending; send: Send; busy: boolean; t: Strings }) {
  // Per question: chosen option labels, and free text that overrides them.
  const [chosen, setChosen] = useState<Record<string, string[]>>({});
  const [other, setOther] = useState<Record<string, string>>({});

  const answerFor = (q: Question) => other[q.question]?.trim() || (chosen[q.question] ?? []).join(", ");
  const complete = request.questions.length > 0 && request.questions.every((q) => answerFor(q) !== "");

  const toggle = (q: Question, label: string) =>
    setChosen((c) => {
      const cur = c[q.question] ?? [];
      const next = q.multiSelect
        ? cur.includes(label)
          ? cur.filter((l) => l !== label)
          : [...cur, label]
        : [label];
      return { ...c, [q.question]: next };
    });

  const submit = () =>
    send({
      action: "answer",
      answers: Object.fromEntries(request.questions.map((q) => [q.question, answerFor(q)])),
    });

  return (
    <form
      className="questions"
      onSubmit={(e) => {
        e.preventDefault();
        if (complete) submit();
      }}
    >
      {request.questions.map((q) => (
        <fieldset key={q.question} className="question">
          <legend>
            {q.header && <span className="question-header">{q.header}</span>}
            <span className="question-text">{q.question}</span>
            {q.multiSelect && <span className="question-hint">Choose any</span>}
          </legend>
          <div className="options">
            {q.options.map((o) => {
              const on = (chosen[q.question] ?? []).includes(o.label) && !other[q.question]?.trim();
              return (
                <button
                  type="button"
                  key={o.label}
                  className={`option${on ? " option-on" : ""}`}
                  aria-pressed={on}
                  onClick={() => toggle(q, o.label)}
                >
                  <span className={`option-mark${q.multiSelect ? " option-mark-multi" : ""}`} aria-hidden="true" />
                  <span className="option-text">
                    <span className="option-label">{o.label}</span>
                    {o.description && <span className="option-description">{o.description}</span>}
                  </span>
                </button>
              );
            })}
          </div>
          <label className="field">
            <span className="visually-hidden">Other answer to: {q.question}</span>
            <input
              type="text"
              value={other[q.question] ?? ""}
              placeholder="Or type an answer…"
              onChange={(e) => setOther((o) => ({ ...o, [q.question]: e.target.value }))}
            />
          </label>
        </fieldset>
      ))}
      <div className="actions">
        <button type="submit" className="btn btn-primary" disabled={busy || !complete}>
          {t.send}
        </button>
        <button
          type="button"
          className="btn"
          disabled={busy}
          onClick={() => send({ action: "deny" })}
          title="Tell Claude you'd rather not answer"
        >
          {t.skip}
        </button>
        <span className="actions-spacer" />
        <button type="button" className="btn btn-link" disabled={busy} onClick={() => send({ action: "terminal" })}>
              {t.useTerminal}
            </button>
      </div>
    </form>
  );
}
