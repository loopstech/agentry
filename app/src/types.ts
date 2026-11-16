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
