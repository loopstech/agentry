// Mirrors agentry_server::store::Session (serialised camelCase).

export type Status =
  | "working"
  | "needs_permission"
  | "needs_answer"
  | "needs_terminal"
  | "idle"
  | "error"
  | "done";

export interface Task {
  id: string;
  subject: string;
  activeForm: string | null;
  status: "pending" | "in_progress" | "completed" | string;
}

export interface Helper {
  agentId: string;
  agentType: string;
  subtitle: string;
  done: boolean;
  startedAt: number;
  endedAt: number | null;
}

export interface QuestionOption {
  label: string;
  description: string | null;
}

export interface Question {
  question: string;
  header: string | null;
  options: QuestionOption[];
  multiSelect: boolean;
}

/** An approval or question waiting in the widget. */
export interface Pending {
  id: string;
  kind: "permission" | "question";
  toolName: string;
  summary: string;
  detail: string | null;
  detailLabel: string | null;
  agentType: string | null;
  questions: Question[];
  alwaysAllowLabel: string | null;
  createdAt: number;
  expiresAt: number | null;
}

export type UiResponse =
  | { action: "allow" }
  | { action: "always_allow" }
  | { action: "deny"; note?: string; stop?: boolean }
  | { action: "terminal" }
  | { action: "answer"; answers: Record<string, string> };

export interface Config {
  approvals: {
    mode: "widget_first" | "terminal_only" | "widget_when_visible";
    timeoutSecs: number | null;
    tools: string[] | null;
    alwaysAllow: boolean;
    answerQuestions: boolean;
    autoOpen: boolean;
  };
  appearance: {
    theme: "mono" | "crew";
    /** "#rrggbb", or null for the theme's own accent. */
    accent: string | null;
    hoverToOpen: boolean;
  };
  sounds: {
    enabled: boolean;
    /** 0–1 */
    volume: number;
  };
  muted: boolean;
}

export interface Session {
  id: string;
  cwd: string;
  project: string;
  title: string;
  titleSource: "claude" | "prompt" | "placeholder";
  status: Status;
  subtitle: string;
  progress: { done: number; total: number; label: string } | null;
  tasks: Task[];
  helpers: Helper[];
  terminal: { ppid: number | null; tty: string | null; env: Record<string, string> };
  transcriptPath: string | null;
  model: string | null;
  permissionMode: string | null;
  avatar: number;
  startedAt: number;
  updatedAt: number;
  endedAt: number | null;
  needsSince: number | null;
  pinned: boolean;
  pending: Pending[];
  /** Picked up from a transcript at launch; no live event yet. */
  backfilled: boolean;
}

export const needsUser = (s: Session) =>
  s.status === "needs_permission" || s.status === "needs_answer" || s.status === "needs_terminal";

export interface Counts {
  needs: number;
  working: number;
  ready: number;
  done: number;
}

export function countSessions(sessions: Session[]): Counts {
  const c: Counts = { needs: 0, working: 0, ready: 0, done: 0 };
  for (const s of sessions) {
    if (needsUser(s)) c.needs++;
    else if (s.status === "working") c.working++;
    else if (s.status === "done") c.done++;
    else c.ready++;
  }
  return c;
}
