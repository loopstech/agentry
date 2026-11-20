import { useState } from "react";
import { needsUser, type Session } from "../types";
import { sendMessage } from "../useSessions";
import { Send } from "./Icons";

/** What the session's avatar is "saying": Claude's latest reply, or typing
 *  dots while it works. Nothing while it waits on you (the request card is
 *  showing) or before it has said anything. */
export function SpeechBubble({ session: s }: { session: Session }) {
  if (needsUser(s)) return null;
  const working = s.status === "working";
  if (!working && !s.said) return null;
  return (
    <div className="bubble" title={working ? undefined : (s.said ?? undefined)}>
      {working ? (
        <span className="bubble-typing" role="img" aria-label="Claude is working">
          <i />
          <i />
          <i />
        </span>
      ) : (
        <span className="bubble-text" aria-label={`Claude said: ${s.said}`}>
          {s.said}
        </span>
      )}
    </div>
  );
}

/** A box that types a message into the session's terminal, as if you'd
 *  typed it there and pressed Return. */
export function MessageBox({
  session: s,
  notify,
}: {
  session: Session;
  notify: (message: string, isError?: boolean) => void;
}) {
  const [text, setText] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const ended = s.status === "done" || s.endedAt !== null;
  const blocked = ended || needsUser(s);
  const placeholder = ended
    ? "This session has ended"
    : needsUser(s)
      ? "Answer Claude's request first"
      : s.status === "working"
        ? "Claude will read this next…"
        : `Message ${s.title}…`;

  const submit = (e: React.FormEvent) => {
    e.preventDefault();
    if (busy || blocked || !text.trim()) return;
    setBusy(true);
    setError(null);
    sendMessage(s.id, text)
      .then((msg) => {
        setText("");
        notify(msg);
      })
      .catch((err) => setError(String(err)))
      .finally(() => setBusy(false));
  };

  return (
    <form className="message-box" onSubmit={submit} aria-busy={busy}>
      <div className="message-field">
        <input
          type="text"
          value={text}
          disabled={blocked || busy}
          placeholder={placeholder}
          aria-label={`Message Claude in ${s.title}`}
          maxLength={4000}
          onChange={(e) => {
            setText(e.target.value);
            if (error) setError(null);
          }}
        />
        <button
          type="submit"
          className="message-send"
          disabled={blocked || busy || !text.trim()}
          aria-label="Send"
          title="Send (↵)"
        >
          <Send />
        </button>
      </div>
      {error && (
        <div className="message-error" role="alert">
          {error}
        </div>
      )}
    </form>
  );
}
