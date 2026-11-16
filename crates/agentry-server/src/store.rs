//! Session state: turns the stream of hook events into one card per Claude
//! Code session.
//!
//! All knowledge of Claude Code payload shapes for display lives here. Shapes
//! were captured from Claude Code 2.1.325; see NOTES.md and the fixtures in
//! `tests/fixtures/`.

use std::collections::{BTreeMap, HashMap, HashSet};

use agentry_core::protocol::{event_name, tool_name, Envelope};
use serde::Serialize;
use serde_json::Value;

/// Finished sessions disappear this long after `SessionEnd`, unless pinned.
pub const DONE_TTL_MS: u64 = 10 * 60 * 1000;
/// Sessions with no events for this long are dropped, unless pinned.
pub const STALE_TTL_MS: u64 = 2 * 60 * 60 * 1000;
/// Finished helpers (subagents) stay visible this long.
pub const HELPER_TTL_MS: u64 = 30 * 1000;

const MAX_SUBTITLE: usize = 72;
const MAX_TITLE: usize = 48;
const TITLE_WORDS: usize = 6;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Working,
    /// Waiting on an approval Agentry can answer (Phase 3).
    NeedsPermission,
    /// Waiting on an answer Agentry can give (Phase 3).
    NeedsAnswer,
    /// Waiting on the user in the terminal.
    NeedsTerminal,
    Idle,
    Error,
    Done,
}

impl Status {
    pub fn needs_user(self) -> bool {
        matches!(
            self,
            Status::NeedsPermission | Status::NeedsAnswer | Status::NeedsTerminal
        )
    }

    /// Sort order: sessions that need you first.
    fn rank(self) -> u8 {
        match self {
            Status::NeedsPermission | Status::NeedsAnswer | Status::NeedsTerminal => 0,
            Status::Working => 1,
            Status::Error => 2,
            Status::Idle => 3,
            Status::Done => 4,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TitleSource {
    /// Claude Code's own session title (or `/rename`).
    Claude,
    /// Derived from the first prompt.
    Prompt,
    /// Nothing to go on yet.
    Placeholder,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Task {
    pub id: String,
    pub subject: String,
    pub active_form: Option<String>,
    /// `pending`, `in_progress` or `completed`.
    pub status: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    pub done: usize,
    pub total: usize,
    pub label: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Helper {
    pub agent_id: String,
    pub agent_type: String,
    pub subtitle: String,
    pub done: bool,
    pub started_at: u64,
    pub ended_at: Option<u64>,
}

/// Where the session runs, for jumping back to it (Phase 4).
#[derive(Debug, Clone, Default, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TerminalInfo {
    pub ppid: Option<u32>,
    pub tty: Option<String>,
    pub env: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    pub id: String,
    pub cwd: String,
    pub project: String,
    pub title: String,
    pub title_source: TitleSource,
    pub status: Status,
    pub subtitle: String,
    pub progress: Option<Progress>,
    pub tasks: Vec<Task>,
    pub helpers: Vec<Helper>,
    pub terminal: TerminalInfo,
    pub transcript_path: Option<String>,
    pub model: Option<String>,
    pub permission_mode: Option<String>,
    /// Stable per session; the theme maps it to avatar art and colour.
    pub avatar: u32,
    pub started_at: u64,
    pub updated_at: u64,
    pub ended_at: Option<u64>,
    /// When the session started waiting on the user.
    pub needs_since: Option<u64>,
    pub pinned: bool,

    /// Last thing the main agent did, e.g. "Reading server.ts".
    #[serde(skip)]
    activity: Option<String>,
    /// Text for the current status when it isn't about work in progress.
    #[serde(skip)]
    note: Option<String>,
}

impl Session {
    fn new(id: &str, cwd: &str, avatar: u32, now: u64) -> Self {
        Session {
            id: id.to_owned(),
            cwd: cwd.to_owned(),
            project: project_name(cwd),
            title: "New session".into(),
            title_source: TitleSource::Placeholder,
            status: Status::Idle,
            subtitle: String::new(),
            progress: None,
            tasks: Vec::new(),
            helpers: Vec::new(),
            terminal: TerminalInfo::default(),
            transcript_path: None,
            model: None,
            permission_mode: None,
            avatar,
            started_at: now,
            updated_at: now,
            ended_at: None,
            needs_since: None,
            pinned: false,
            activity: None,
            note: Some("Ready".into()),
        }
    }

    fn set_status(&mut self, status: Status, note: Option<String>, now: u64) {
        if status.needs_user() {
            if !self.status.needs_user() {
                self.needs_since = Some(now);
            }
        } else {
            self.needs_since = None;
        }
        if status != Status::Done {
            self.ended_at = None;
        }
        self.status = status;
        self.note = note;
    }

    fn task_mut(&mut self, id: &str) -> Option<&mut Task> {
        self.tasks.iter_mut().find(|t| t.id == id)
    }

    fn upsert_task(
        &mut self,
        id: &str,
        subject: Option<&str>,
        active_form: Option<&str>,
        status: Option<&str>,
    ) {
        if status == Some("deleted") {
            self.tasks.retain(|t| t.id != id);
            return;
        }
        if self.task_mut(id).is_none() {
            self.tasks.push(Task {
                id: id.to_owned(),
                subject: subject.unwrap_or("Task").to_owned(),
                active_form: None,
                status: "pending".into(),
            });
        }
        let task = self.task_mut(id).expect("just inserted");
        if let Some(s) = subject.filter(|s| !s.is_empty()) {
            task.subject = s.to_owned();
        }
        if let Some(a) = active_form.filter(|a| !a.is_empty()) {
            task.active_form = Some(a.to_owned());
        }
        if let Some(s) = status {
            task.status = s.to_owned();
        }
    }

    /// Recompute the derived fields shown on the card.
    fn refresh(&mut self) {
        let total = self.tasks.len();
        self.progress = (total > 0).then(|| Progress {
            done: self
                .tasks
                .iter()
                .filter(|t| t.status == "completed")
                .count(),
            total,
            label: "tasks".into(),
        });

        let current_task = self
            .tasks
            .iter()
            .find(|t| t.status == "in_progress")
            .map(|t| t.active_form.clone().unwrap_or_else(|| t.subject.clone()));

        self.subtitle = truncate(
            &match self.status {
                Status::Working => current_task
                    .or_else(|| self.activity.clone())
                    .unwrap_or_else(|| "Working…".into()),
                _ => self.note.clone().unwrap_or_default(),
            },
            MAX_SUBTITLE,
        );
    }
}

/// All live sessions.
#[derive(Debug, Default)]
pub struct Store {
    sessions: HashMap<String, Session>,
    /// Sessions the user dismissed; they come back when the user prompts again.
    dismissed: HashSet<String>,
    next_avatar: u32,
}

impl Store {
    pub fn new() -> Self {
        Self::default()
    }

    /// Apply one hook event. Returns whether anything visible changed.
    pub fn apply(&mut self, envelope: &Envelope, now: u64) -> bool {
        let p = &envelope.payload;
        let (Some(event), Some(sid)) = (event_name(p), str_field(p, "session_id")) else {
            return false;
        };

        if self.dismissed.contains(sid) {
            match event {
                "SessionStart" | "UserPromptSubmit" => {
                    self.dismissed.remove(sid);
                }
                "SessionEnd" => {
                    self.dismissed.remove(sid);
                    return false;
                }
                _ => return false,
            }
        }

        if !self.sessions.contains_key(sid) {
            let new = Session::new(
                sid,
                str_field(p, "cwd").unwrap_or(""),
                self.next_avatar,
                now,
            );
            self.sessions.insert(sid.to_owned(), new);
            self.next_avatar += 1;
        }
        let s = self.sessions.get_mut(sid).expect("just inserted");

        s.updated_at = now;
        if s.cwd.is_empty() {
            if let Some(cwd) = str_field(p, "cwd") {
                s.cwd = cwd.to_owned();
                s.project = project_name(cwd);
            }
        }
        if let Some(t) = str_field(p, "transcript_path") {
            s.transcript_path = Some(t.to_owned());
        }
        if let Some(m) = str_field(p, "permission_mode") {
            s.permission_mode = Some(m.to_owned());
        }
        merge_terminal(&mut s.terminal, envelope);

        let agent_id = str_field(p, "agent_id");
        match event {
            "SessionStart" => {
                if str_field(p, "source") == Some("clear") {
                    s.tasks.clear();
                    s.helpers.clear();
                    s.activity = None;
                }
                if let Some(t) = str_field(p, "session_title").filter(|t| !t.trim().is_empty()) {
                    s.title = truncate(t.trim(), MAX_TITLE);
                    s.title_source = TitleSource::Claude;
                }
                if let Some(m) = str_field(p, "model") {
                    s.model = Some(m.to_owned());
                }
                if s.status != Status::Working {
                    s.set_status(Status::Idle, Some("Ready".into()), now);
                }
            }

            "UserPromptSubmit" => {
                let prompt = str_field(p, "prompt").unwrap_or("").trim();
                if let Some(name) = prompt
                    .strip_prefix("/rename ")
                    .map(str::trim)
                    .filter(|n| !n.is_empty())
                {
                    s.title = truncate(name, MAX_TITLE);
                    s.title_source = TitleSource::Claude;
                } else if s.title_source == TitleSource::Placeholder
                    && !prompt.is_empty()
                    && !prompt.starts_with('/')
                {
                    if let Some(t) = title_from_prompt(prompt) {
                        s.title = t;
                        s.title_source = TitleSource::Prompt;
                    }
                }
                s.set_status(Status::Working, None, now);
                s.activity = Some("Thinking…".into());
            }

            "PreToolUse" if tool_name(p) == Some("AskUserQuestion") => {
                let q = p
                    .pointer("/tool_input/questions/0/question")
                    .and_then(Value::as_str)
                    .unwrap_or("Has a question for you");
                s.set_status(Status::NeedsTerminal, Some(format!("Asks: {q}")), now);
            }

            "PermissionRequest" => {
                let what = tool_name(p)
                    .map(|t| describe_request(t, p.get("tool_input").unwrap_or(&Value::Null)))
                    .unwrap_or_else(|| "Needs your permission".into());
                s.set_status(Status::NeedsTerminal, Some(what), now);
            }

            "PostToolUse" | "PostToolUseFailure" => {
                let tool = tool_name(p).unwrap_or("");
                let input = p.get("tool_input").unwrap_or(&Value::Null);
                match agent_id {
                    Some(aid) => {
                        let summary = describe_tool(tool, input);
                        let helper = helper_entry(s, aid, str_field(p, "agent_type"), now);
                        if let Some(summary) = summary {
                            helper.subtitle = summary;
                        }
                    }
                    None => {
                        apply_task_tool(
                            s,
                            tool,
                            input,
                            p.get("tool_response").unwrap_or(&Value::Null),
                        );
                        if let Some(summary) = describe_tool(tool, input) {
                            s.activity = Some(summary);
                        }
                    }
                }
                if s.status != Status::Working && s.status != Status::Done {
                    s.set_status(Status::Working, None, now);
                }
            }

            "TaskCreated" | "TaskCompleted" if agent_id.is_none() => {
                if let Some(id) = str_field(p, "task_id") {
                    let status = (event == "TaskCompleted").then_some("completed");
                    s.upsert_task(id, str_field(p, "task_subject"), None, status);
                }
            }

            "SubagentStart" => {
                if let Some(aid) = agent_id {
                    let kind = helper_entry(s, aid, str_field(p, "agent_type"), now)
                        .agent_type
                        .clone();
                    s.activity = Some(format!("Working with {kind}"));
                }
            }

            "SubagentStop" => {
                if let Some(aid) = agent_id {
                    let h = helper_entry(s, aid, str_field(p, "agent_type"), now);
                    h.done = true;
                    h.ended_at = Some(now);
                    h.subtitle = "Done".into();
                }
            }

            "Notification" => match str_field(p, "notification_type") {
                Some(
                    "permission_prompt"
                    | "agent_needs_input"
                    | "elicitation_dialog"
                    | "elicitation_url_dialog",
                ) if !s.status.needs_user() => {
                    let msg = str_field(p, "message").unwrap_or("Needs you in the terminal");
                    s.set_status(Status::NeedsTerminal, Some(msg.to_owned()), now);
                }
                _ => {}
            },

            "Stop" if agent_id.is_none() => {
                for h in s.helpers.iter_mut().filter(|h| !h.done) {
                    h.done = true;
                    h.ended_at = Some(now);
                    h.subtitle = "Done".into();
                }
                s.set_status(Status::Idle, Some("Finished — ready for you".into()), now);
            }

            "StopFailure" => {
                s.set_status(Status::Error, Some("Stopped with an error".into()), now);
            }

            "SessionEnd" => {
                s.set_status(Status::Done, Some("Session ended".into()), now);
                s.ended_at = Some(now);
            }

            _ => {}
        }

        s.refresh();
        true
    }

    /// Drop expired sessions and helpers. Returns whether anything changed.
    pub fn tick(&mut self, now: u64) -> bool {
        let before = self.sessions.len();
        self.sessions.retain(|_, s| {
            s.pinned
                || !(s
                    .ended_at
                    .is_some_and(|t| now.saturating_sub(t) > DONE_TTL_MS)
                    || now.saturating_sub(s.updated_at) > STALE_TTL_MS)
        });
        let mut changed = self.sessions.len() != before;
        for s in self.sessions.values_mut() {
            let n = s.helpers.len();
            s.helpers.retain(|h| {
                !h.ended_at
                    .is_some_and(|t| now.saturating_sub(t) > HELPER_TTL_MS)
            });
            changed |= s.helpers.len() != n;
        }
        changed
    }

    /// Sessions in display order: needs-you first, then by start time.
    pub fn snapshot(&self) -> Vec<Session> {
        let mut v: Vec<Session> = self.sessions.values().cloned().collect();
        v.sort_by(|a, b| {
            a.status
                .rank()
                .cmp(&b.status.rank())
                .then(a.started_at.cmp(&b.started_at))
                .then(a.id.cmp(&b.id))
        });
        v
    }

    pub fn get(&self, id: &str) -> Option<&Session> {
        self.sessions.get(id)
    }

    /// Remove a session from view until it's prompted again.
    pub fn dismiss(&mut self, id: &str) -> bool {
        if self.sessions.remove(id).is_some() {
            self.dismissed.insert(id.to_owned());
            true
        } else {
            false
        }
    }

    pub fn set_pinned(&mut self, id: &str, pinned: bool) -> bool {
        match self.sessions.get_mut(id) {
            Some(s) if s.pinned != pinned => {
                s.pinned = pinned;
                true
            }
            _ => false,
        }
    }
}

fn str_field<'a>(v: &'a Value, key: &str) -> Option<&'a str> {
    v.get(key)?.as_str()
}

fn helper_entry<'a>(
    s: &'a mut Session,
    agent_id: &str,
    agent_type: Option<&str>,
    now: u64,
) -> &'a mut Helper {
    if let Some(i) = s.helpers.iter().position(|h| h.agent_id == agent_id) {
        return &mut s.helpers[i];
    }
    s.helpers.push(Helper {
        agent_id: agent_id.to_owned(),
        agent_type: agent_type.unwrap_or("Helper").to_owned(),
        subtitle: "Starting…".into(),
        done: false,
        started_at: now,
        ended_at: None,
    });
    s.helpers.last_mut().expect("just pushed")
}

fn merge_terminal(t: &mut TerminalInfo, envelope: &Envelope) {
    let c = &envelope.context;
    if c.ppid.is_some() {
        t.ppid = c.ppid;
    }
    if c.tty.is_some() {
        t.tty = c.tty.clone();
    }
    if !c.env.is_empty() {
        t.env = c.env.clone();
    }
}

/// Track tasks from `TaskCreate`/`TaskUpdate` (current Claude Code) and
/// `TodoWrite` (older versions).
fn apply_task_tool(s: &mut Session, tool: &str, input: &Value, response: &Value) {
    match tool {
        "TaskCreate" => {
            let id = response.pointer("/task/id").and_then(Value::as_str);
            if let Some(id) = id {
                s.upsert_task(
                    id,
                    str_field(input, "subject"),
                    str_field(input, "activeForm"),
                    None,
                );
            }
        }
        "TaskUpdate" => {
            if let Some(id) = str_field(input, "taskId").or_else(|| str_field(response, "taskId")) {
                s.upsert_task(
                    id,
                    str_field(input, "subject"),
                    str_field(input, "activeForm"),
                    str_field(input, "status"),
                );
            }
        }
        "TodoWrite" => {
            if let Some(todos) = input.get("todos").and_then(Value::as_array) {
                s.tasks = todos
                    .iter()
                    .enumerate()
                    .map(|(i, t)| Task {
                        id: str_field(t, "id")
                            .map(str::to_owned)
                            .unwrap_or_else(|| format!("todo-{i}")),
                        subject: str_field(t, "content").unwrap_or("Task").to_owned(),
                        active_form: str_field(t, "activeForm").map(str::to_owned),
                        status: str_field(t, "status").unwrap_or("pending").to_owned(),
                    })
                    .collect();
            }
        }
        _ => {}
    }
}

/// A short "what it's doing" line for a finished tool call, or `None` for
/// bookkeeping tools that shouldn't replace the current activity.
pub fn describe_tool(tool: &str, input: &Value) -> Option<String> {
    let file = || {
        str_field(input, "file_path")
            .or_else(|| str_field(input, "notebook_path"))
            .map(basename)
    };
    let text = match tool {
        "TaskCreate" | "TaskUpdate" | "TaskList" | "TaskGet" | "TodoWrite" | "ToolSearch"
        | "AskUserQuestion" | "ExitPlanMode" | "EnterPlanMode" => return None,
        "Read" => format!("Reading {}", file().unwrap_or("a file")),
        "Write" => format!("Writing {}", file().unwrap_or("a file")),
        "Edit" | "MultiEdit" | "NotebookEdit" => format!("Editing {}", file().unwrap_or("a file")),
        "Bash" => match str_field(input, "description").filter(|d| !d.trim().is_empty()) {
            Some(d) => d.trim().to_owned(),
            None => {
                let cmd = str_field(input, "command").unwrap_or("").trim();
                let first = cmd.split_whitespace().next().unwrap_or("a command");
                format!("Running {}", basename(first))
            }
        },
        "Grep" => match str_field(input, "pattern") {
            Some(p) => format!("Searching for “{}”", truncate(p, 28)),
            None => "Searching the code".into(),
        },
        "Glob" => "Finding files".into(),
        "WebFetch" => match str_field(input, "url").and_then(host_of) {
            Some(h) => format!("Reading {h}"),
            None => "Reading a web page".into(),
        },
        "WebSearch" => "Searching the web".into(),
        "Agent" | "Task" => match str_field(input, "subagent_type") {
            Some(t) => format!("Working with {t}"),
            None => "Working with a helper".into(),
        },
        t if t.starts_with("mcp__") => {
            let server = t
                .trim_start_matches("mcp__")
                .split("__")
                .next()
                .unwrap_or("a tool");
            format!("Using {server}")
        }
        "" => return None,
        t => format!("Using {t}"),
    };
    Some(truncate(&text, MAX_SUBTITLE))
}

/// What a permission request is for, e.g. "Wants to run: pnpm add -D x".
pub fn describe_request(tool: &str, input: &Value) -> String {
    let file = || {
        str_field(input, "file_path")
            .map(basename)
            .unwrap_or("a file")
    };
    let text = match tool {
        "Bash" => format!(
            "Wants to run: {}",
            str_field(input, "command").unwrap_or("a command").trim()
        ),
        "Write" => format!("Wants to write {}", file()),
        "Edit" | "MultiEdit" | "NotebookEdit" => format!("Wants to edit {}", file()),
        "Read" => format!(
            "Wants to read {}",
            str_field(input, "file_path").unwrap_or("a file")
        ),
        "WebFetch" => format!(
            "Wants to open {}",
            str_field(input, "url")
                .and_then(host_of)
                .unwrap_or_else(|| "a web page".into())
        ),
        t => format!("Wants to use {t}"),
    };
    truncate(&text, MAX_SUBTITLE)
}

/// "Can you fix the login bug please?" → "Fix the login bug please".
pub fn title_from_prompt(prompt: &str) -> Option<String> {
    let line = prompt.lines().map(str::trim).find(|l| !l.is_empty())?;
    let fillers = [
        "please", "can", "could", "would", "you", "hey", "hi", "ok", "okay", "so",
    ];
    let mut words: Vec<&str> = line.split_whitespace().collect();
    while words.len() > 1
        && fillers.contains(
            &words[0]
                .trim_matches(|c: char| !c.is_alphanumeric())
                .to_lowercase()
                .as_str(),
        )
    {
        words.remove(0);
    }
    // Stop at the end of the first clause when that still leaves a few words.
    let clause_end = words
        .iter()
        .position(|w| w.ends_with([',', '.', ';', ':', '!', '?']))
        .filter(|&i| i >= 2);
    let take = clause_end.map_or(TITLE_WORDS, |i| (i + 1).min(TITLE_WORDS));
    let mut words: Vec<&str> = words.into_iter().take(take).collect();
    // Don't end on a dangling "to", "the", "and"…
    let dangling = [
        "a", "an", "the", "to", "and", "or", "for", "of", "with", "in", "on", "at", "by", "from",
        "into", "that", "so",
    ];
    while words.len() > 1 && dangling.contains(&words[words.len() - 1].to_lowercase().as_str()) {
        words.pop();
    }
    let mut title = words.join(" ");
    title = title
        .trim_end_matches(|c: char| ".,;:!?…-—".contains(c))
        .to_owned();
    if title.is_empty() {
        return None;
    }
    let mut chars = title.chars();
    let first = chars.next()?.to_uppercase().collect::<String>();
    Some(truncate(&(first + chars.as_str()), MAX_TITLE))
}

fn project_name(cwd: &str) -> String {
    let name = basename(cwd.trim_end_matches('/'));
    if name.is_empty() {
        "~".into()
    } else {
        name.to_owned()
    }
}

fn basename(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

fn host_of(url: &str) -> Option<String> {
    let rest = url.split_once("://").map(|(_, r)| r).unwrap_or(url);
    let host = rest
        .split(['/', '?', '#'])
        .next()?
        .trim_start_matches("www.");
    (!host.is_empty()).then(|| host.to_owned())
}

/// Shorten to `max` characters, ending in "…" when cut.
pub fn truncate(s: &str, max: usize) -> String {
    let s = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if s.chars().count() <= max {
        return s;
    }
    let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
    out = out.trim_end().to_owned();
    out.push('…');
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentry_core::protocol::HookContext;
    use serde_json::json;

    fn env(payload: Value) -> Envelope {
        Envelope {
            payload,
            context: HookContext::default(),
        }
    }

    fn ev(store: &mut Store, now: u64, payload: Value) {
        store.apply(&env(payload), now);
    }

    #[test]
    fn titles_from_prompts() {
        assert_eq!(
            title_from_prompt("can you fix the login bug please?").as_deref(),
            Some("Fix the login bug please")
        );
        assert_eq!(
            title_from_prompt("Refactor the payment service to use the new client and add tests")
                .as_deref(),
            Some("Refactor the payment service to use")
        );
        assert_eq!(
            title_from_prompt("\n\n  add dark mode.\nmore").as_deref(),
            Some("Add dark mode")
        );
        assert_eq!(title_from_prompt("   ").as_deref(), None);
        assert_eq!(title_from_prompt("please").as_deref(), Some("Please"));
        assert_eq!(
            title_from_prompt("Create 4 tasks with TaskCreate, each with an activeForm").as_deref(),
            Some("Create 4 tasks with TaskCreate")
        );
        assert_eq!(title_from_prompt("Hi, fix it").as_deref(), Some("Fix it"));
        assert_eq!(
            title_from_prompt("Add a dark mode toggle to the settings page").as_deref(),
            Some("Add a dark mode toggle")
        );
        assert_eq!(
            title_from_prompt("Add a, b and c").as_deref(),
            Some("Add a, b and c")
        );
    }

    #[test]
    fn tool_descriptions() {
        assert_eq!(
            describe_tool("Read", &json!({"file_path": "/a/b/server.ts"})).unwrap(),
            "Reading server.ts"
        );
        assert_eq!(
            describe_tool(
                "Bash",
                &json!({"command": "pnpm test", "description": "Run the tests"})
            )
            .unwrap(),
            "Run the tests"
        );
        assert_eq!(
            describe_tool("Bash", &json!({"command": "/usr/bin/make all"})).unwrap(),
            "Running make"
        );
        assert_eq!(
            describe_tool("WebFetch", &json!({"url": "https://www.example.com/x"})).unwrap(),
            "Reading example.com"
        );
        assert_eq!(
            describe_tool("mcp__github__get_issue", &json!({})).unwrap(),
            "Using github"
        );
        assert_eq!(
            describe_tool("Agent", &json!({"subagent_type": "Explore"})).unwrap(),
            "Working with Explore"
        );
        assert!(describe_tool("TaskUpdate", &json!({})).is_none());
        assert_eq!(
            describe_request("Bash", &json!({"command": "rm -rf dist"})),
            "Wants to run: rm -rf dist"
        );
    }

    #[test]
    fn truncation() {
        assert_eq!(truncate("short", 10), "short");
        assert_eq!(truncate("a  b\n c", 10), "a b c");
        assert_eq!(truncate("abcdefghijkl", 6), "abcde…");
    }

    #[test]
    fn lifecycle() {
        let mut s = Store::new();
        ev(
            &mut s,
            1,
            json!({"hook_event_name": "SessionStart", "session_id": "a", "cwd": "/w/api-server", "source": "startup"}),
        );
        let a = &s.snapshot()[0];
        assert_eq!(
            (
                a.project.as_str(),
                a.title.as_str(),
                a.status,
                a.subtitle.as_str()
            ),
            ("api-server", "New session", Status::Idle, "Ready")
        );

        ev(
            &mut s,
            2,
            json!({"hook_event_name": "UserPromptSubmit", "session_id": "a", "prompt": "fix the auth flow"}),
        );
        let a = &s.snapshot()[0];
        assert_eq!(
            (a.title.as_str(), a.status, a.subtitle.as_str()),
            ("Fix the auth flow", Status::Working, "Thinking…")
        );

        ev(
            &mut s,
            3,
            json!({"hook_event_name": "PostToolUse", "session_id": "a", "tool_name": "Read", "tool_input": {"file_path": "/w/api-server/auth.ts"}}),
        );
        assert_eq!(s.snapshot()[0].subtitle, "Reading auth.ts");

        ev(
            &mut s,
            4,
            json!({"hook_event_name": "PermissionRequest", "session_id": "a", "tool_name": "Bash", "tool_input": {"command": "pnpm i"}}),
        );
        let a = &s.snapshot()[0];
        assert_eq!(
            (a.status, a.subtitle.as_str(), a.needs_since),
            (Status::NeedsTerminal, "Wants to run: pnpm i", Some(4))
        );

        ev(
            &mut s,
            5,
            json!({"hook_event_name": "PostToolUse", "session_id": "a", "tool_name": "Bash", "tool_input": {"command": "pnpm i"}}),
        );
        let a = &s.snapshot()[0];
        assert_eq!((a.status, a.needs_since), (Status::Working, None));

        // A second prompt doesn't change the title.
        ev(
            &mut s,
            6,
            json!({"hook_event_name": "UserPromptSubmit", "session_id": "a", "prompt": "now something else"}),
        );
        assert_eq!(s.snapshot()[0].title, "Fix the auth flow");

        ev(
            &mut s,
            7,
            json!({"hook_event_name": "Stop", "session_id": "a"}),
        );
        assert_eq!(s.snapshot()[0].subtitle, "Finished — ready for you");

        ev(
            &mut s,
            8,
            json!({"hook_event_name": "SessionEnd", "session_id": "a", "reason": "other"}),
        );
        assert_eq!(s.snapshot()[0].status, Status::Done);
        assert!(!s.tick(8 + DONE_TTL_MS));
        assert!(s.tick(9 + DONE_TTL_MS));
        assert!(s.snapshot().is_empty());
    }

    #[test]
    fn rename_and_session_title() {
        let mut s = Store::new();
        ev(
            &mut s,
            1,
            json!({"hook_event_name": "SessionStart", "session_id": "a", "cwd": "/w/x", "session_title": "Release prep"}),
        );
        assert_eq!(s.snapshot()[0].title, "Release prep");
        ev(
            &mut s,
            2,
            json!({"hook_event_name": "UserPromptSubmit", "session_id": "a", "prompt": "write notes"}),
        );
        assert_eq!(s.snapshot()[0].title, "Release prep");
        ev(
            &mut s,
            3,
            json!({"hook_event_name": "UserPromptSubmit", "session_id": "a", "prompt": "/rename Ship v2"}),
        );
        assert_eq!(s.snapshot()[0].title, "Ship v2");
    }

    #[test]
    fn sessions_seen_mid_way_are_created() {
        let mut s = Store::new();
        ev(
            &mut s,
            1,
            json!({"hook_event_name": "PostToolUse", "session_id": "late", "cwd": "/w/web", "tool_name": "Glob", "tool_input": {}}),
        );
        let a = &s.snapshot()[0];
        assert_eq!(
            (a.project.as_str(), a.status, a.subtitle.as_str()),
            ("web", Status::Working, "Finding files")
        );
    }

    #[test]
    fn ordering_needs_first_then_start_time() {
        let mut s = Store::new();
        for (t, id) in [(1, "a"), (2, "b"), (3, "c")] {
            ev(
                &mut s,
                t,
                json!({"hook_event_name": "UserPromptSubmit", "session_id": id, "cwd": format!("/w/{id}"), "prompt": "go"}),
            );
        }
        ev(
            &mut s,
            4,
            json!({"hook_event_name": "Stop", "session_id": "a"}),
        );
        ev(
            &mut s,
            5,
            json!({"hook_event_name": "Notification", "session_id": "c", "notification_type": "permission_prompt", "message": "Claude needs your permission to use Bash"}),
        );
        let ids: Vec<_> = s.snapshot().into_iter().map(|x| x.id).collect();
        assert_eq!(ids, ["c", "b", "a"]);
        assert_eq!(
            s.get("c").unwrap().subtitle,
            "Claude needs your permission to use Bash"
        );
        assert_eq!(
            s.snapshot().iter().map(|x| x.avatar).collect::<Vec<_>>(),
            [2, 1, 0]
        );
    }

    #[test]
    fn dismissed_sessions_return_on_next_prompt() {
        let mut s = Store::new();
        ev(
            &mut s,
            1,
            json!({"hook_event_name": "UserPromptSubmit", "session_id": "a", "cwd": "/w/a", "prompt": "x"}),
        );
        assert!(s.dismiss("a"));
        ev(
            &mut s,
            2,
            json!({"hook_event_name": "PostToolUse", "session_id": "a", "tool_name": "Read", "tool_input": {}}),
        );
        assert!(s.snapshot().is_empty());
        ev(
            &mut s,
            3,
            json!({"hook_event_name": "UserPromptSubmit", "session_id": "a", "prompt": "y"}),
        );
        assert_eq!(s.snapshot().len(), 1);
    }

    #[test]
    fn pinned_sessions_survive_expiry() {
        let mut s = Store::new();
        ev(
            &mut s,
            1,
            json!({"hook_event_name": "SessionEnd", "session_id": "a", "cwd": "/w/a"}),
        );
        assert!(s.set_pinned("a", true));
        s.tick(1 + STALE_TTL_MS * 2);
        assert_eq!(s.snapshot().len(), 1);
    }

    #[test]
    fn todowrite_progress() {
        let mut s = Store::new();
        ev(
            &mut s,
            1,
            json!({"hook_event_name": "PostToolUse", "session_id": "a", "cwd": "/w/a", "tool_name": "TodoWrite",
            "tool_input": {"todos": [
                {"content": "Write tests", "status": "completed", "activeForm": "Writing tests"},
                {"content": "Fix bug", "status": "in_progress", "activeForm": "Fixing the bug"},
                {"content": "Ship", "status": "pending"}]}}),
        );
        let a = &s.snapshot()[0];
        assert_eq!(
            a.progress,
            Some(Progress {
                done: 1,
                total: 3,
                label: "tasks".into()
            })
        );
        assert_eq!(a.subtitle, "Fixing the bug");
    }

    #[test]
    fn task_deletion() {
        let mut s = Store::new();
        ev(
            &mut s,
            1,
            json!({"hook_event_name": "TaskCreated", "session_id": "a", "cwd": "/w/a", "task_id": "1", "task_subject": "One"}),
        );
        ev(
            &mut s,
            2,
            json!({"hook_event_name": "TaskCreated", "session_id": "a", "task_id": "2", "task_subject": "Two"}),
        );
        ev(
            &mut s,
            3,
            json!({"hook_event_name": "PostToolUse", "session_id": "a", "tool_name": "TaskUpdate",
            "tool_input": {"taskId": "2", "status": "deleted"}, "tool_response": {"success": true}}),
        );
        assert_eq!(s.snapshot()[0].progress.as_ref().unwrap().total, 1);
    }

    #[test]
    fn subagent_tasks_do_not_count_for_parent() {
        let mut s = Store::new();
        ev(
            &mut s,
            1,
            json!({"hook_event_name": "TaskCreated", "session_id": "a", "cwd": "/w/a", "agent_id": "x", "task_id": "1", "task_subject": "Sub"}),
        );
        assert!(s.snapshot()[0].progress.is_none());
    }
}
