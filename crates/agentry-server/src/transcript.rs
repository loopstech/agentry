//! Reading Claude Code session transcripts (`~/.claude/projects/*/<id>.jsonl`).
//!
//! Used for the detail view (recent steps), for Claude Code's own session
//! titles (`ai-title`, `custom-title` lines) and for picking up sessions that
//! were already running when Agentry started. Files are read incrementally:
//! each poll reads only what was appended since the last one.

use std::collections::HashMap;
use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;
use serde_json::Value;

use crate::store::{describe_tool, truncate};

/// Steps kept per transcript.
const MAX_STEPS: usize = 200;
/// Read at most this much of a large transcript's tail on first open.
const MAX_INITIAL_READ: u64 = 4 * 1024 * 1024;
/// And at most this much per poll.
const MAX_POLL_READ: u64 = 8 * 1024 * 1024;
const MAX_TEXT: usize = 280;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StepKind {
    /// Something the user typed.
    Prompt,
    /// Claude's reply text.
    Text,
    /// A tool call.
    Tool,
    /// A notice, e.g. an interruption.
    Note,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolStatus {
    Running,
    Ok,
    Error,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Step {
    /// Position in the transcript; stable across polls.
    pub seq: usize,
    pub kind: StepKind,
    pub text: String,
    pub tool: Option<String>,
    pub status: Option<ToolStatus>,
    /// Unix milliseconds.
    pub at: Option<u64>,
}

/// Facts about the session found in its transcript.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TranscriptInfo {
    pub session_id: Option<String>,
    pub cwd: Option<String>,
    /// Set by `/rename`.
    pub custom_title: Option<String>,
    /// Claude Code's generated title.
    pub ai_title: Option<String>,
    pub first_prompt: Option<String>,
}

#[derive(Debug)]
pub struct Transcript {
    path: PathBuf,
    offset: u64,
    partial: Vec<u8>,
    /// Set when reading starts mid-file: the first line read is cut off.
    skip_first_line: bool,
    steps: Vec<Step>,
    next_seq: usize,
    /// tool_use_id → index into `steps`.
    tools: HashMap<String, usize>,
    pub info: TranscriptInfo,
}

impl Transcript {
    /// Start reading `path`. Only `.jsonl` files are accepted.
    pub fn open(path: &Path) -> io::Result<Transcript> {
        if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "not a transcript (.jsonl) file",
            ));
        }
        let mut t = Self::empty(path);
        let len = std::fs::metadata(path)?.len();
        if len > MAX_INITIAL_READ {
            // Start near the end of a large transcript.
            t.offset = len - MAX_INITIAL_READ;
            t.skip_first_line = true;
        }
        t.poll()?;
        Ok(t)
    }

    fn empty(path: &Path) -> Transcript {
        Transcript {
            path: path.to_path_buf(),
            offset: 0,
            partial: Vec::new(),
            skip_first_line: false,
            steps: Vec::new(),
            next_seq: 0,
            tools: HashMap::new(),
            info: TranscriptInfo::default(),
        }
    }

    /// Read anything appended since the last poll. Returns whether anything new
    /// was parsed.
    pub fn poll(&mut self) -> io::Result<bool> {
        let mut f = File::open(&self.path)?;
        let len = f.metadata()?.len();
        if len < self.offset {
            // Truncated or replaced: start over.
            *self = Self::empty(&self.path);
        }
        if len == self.offset {
            return Ok(false);
        }
        f.seek(SeekFrom::Start(self.offset))?;
        let mut buf = Vec::new();
        f.take(MAX_POLL_READ).read_to_end(&mut buf)?;
        self.offset += buf.len() as u64;

        let mut data = std::mem::take(&mut self.partial);
        data.extend_from_slice(&buf);
        let complete = match data.iter().rposition(|&b| b == b'\n') {
            Some(i) => i + 1,
            None => {
                self.partial = data;
                return Ok(false);
            }
        };
        self.partial = data[complete..].to_vec();

        let mut lines = data[..complete].split(|&b| b == b'\n');
        if std::mem::take(&mut self.skip_first_line) {
            lines.next();
        }
        let mut parsed = false;
        for line in lines.filter(|l| !l.is_empty()) {
            if let Ok(v) = serde_json::from_slice::<Value>(line) {
                self.ingest(&v);
                parsed = true;
            }
        }
        if self.steps.len() > MAX_STEPS {
            let drop = self.steps.len() - MAX_STEPS;
            self.steps.drain(..drop);
            self.tools.retain(|_, i| *i >= drop);
            for i in self.tools.values_mut() {
                *i -= drop;
            }
        }
        Ok(parsed)
    }

    /// The last `n` steps, oldest first.
    pub fn recent(&self, n: usize) -> Vec<Step> {
        self.steps[self.steps.len().saturating_sub(n)..].to_vec()
    }

    fn push(
        &mut self,
        kind: StepKind,
        text: String,
        tool: Option<String>,
        status: Option<ToolStatus>,
        at: Option<u64>,
    ) -> usize {
        self.steps.push(Step {
            seq: self.next_seq,
            kind,
            text,
            tool,
            status,
            at,
        });
        self.next_seq += 1;
        self.steps.len() - 1
    }

    fn ingest(&mut self, v: &Value) {
        let s = |k: &str| v.get(k).and_then(Value::as_str);
        if self.info.session_id.is_none() {
            self.info.session_id = s("sessionId").map(str::to_owned);
        }
        if self.info.cwd.is_none() {
            self.info.cwd = s("cwd").map(str::to_owned);
        }
        let at = s("timestamp").and_then(parse_timestamp);
        let sidechain = v
            .get("isSidechain")
            .and_then(Value::as_bool)
            .unwrap_or(false);

        match s("type") {
            Some("ai-title") => {
                if let Some(t) = s("aiTitle").filter(|t| !t.trim().is_empty()) {
                    self.info.ai_title = Some(t.trim().to_owned());
                }
            }
            Some("custom-title") => {
                if let Some(t) = s("customTitle")
                    .or_else(|| s("title"))
                    .filter(|t| !t.trim().is_empty())
                {
                    self.info.custom_title = Some(t.trim().to_owned());
                }
            }
            Some("summary") => {
                if self.info.ai_title.is_none() {
                    self.info.ai_title = s("summary").map(str::to_owned);
                }
            }
            Some("user") if !sidechain => {
                let content = v.pointer("/message/content");
                match content {
                    Some(Value::String(text)) => self.user_text(text, at),
                    Some(Value::Array(items)) => {
                        for item in items {
                            match item.get("type").and_then(Value::as_str) {
                                Some("tool_result") => {
                                    let id = item
                                        .get("tool_use_id")
                                        .and_then(Value::as_str)
                                        .unwrap_or("");
                                    let error = item
                                        .get("is_error")
                                        .and_then(Value::as_bool)
                                        .unwrap_or(false);
                                    if let Some(&i) = self.tools.get(id) {
                                        self.steps[i].status = Some(if error {
                                            ToolStatus::Error
                                        } else {
                                            ToolStatus::Ok
                                        });
                                    }
                                }
                                Some("text") => {
                                    if let Some(t) = item.get("text").and_then(Value::as_str) {
                                        self.user_text(t, at);
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                    _ => {}
                }
            }
            Some("assistant") if !sidechain => {
                for item in v
                    .pointer("/message/content")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                {
                    match item.get("type").and_then(Value::as_str) {
                        Some("text") => {
                            let t =
                                plain_text(item.get("text").and_then(Value::as_str).unwrap_or(""));
                            if !t.is_empty() {
                                self.push(StepKind::Text, truncate(&t, MAX_TEXT), None, None, at);
                            }
                        }
                        Some("tool_use") => {
                            let name = item.get("name").and_then(Value::as_str).unwrap_or("tool");
                            let input = item.get("input").unwrap_or(&Value::Null);
                            let text = match name {
                                "ToolSearch" => continue,
                                "TaskCreate" | "TaskUpdate" | "TodoWrite" | "TaskList"
                                | "TaskGet" => "Updated the task list".to_owned(),
                                "AskUserQuestion" => "Asked you a question".to_owned(),
                                _ => describe_tool(name, input)
                                    .unwrap_or_else(|| format!("Used {name}")),
                            };
                            let i = self.push(
                                StepKind::Tool,
                                text,
                                Some(name.to_owned()),
                                Some(ToolStatus::Running),
                                at,
                            );
                            if let Some(id) = item.get("id").and_then(Value::as_str) {
                                self.tools.insert(id.to_owned(), i);
                            }
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }

    fn user_text(&mut self, text: &str, at: Option<u64>) {
        let t = text.trim();
        if t.is_empty() || t.starts_with('<') {
            // System reminders, command output and the like.
            return;
        }
        if t.starts_with("[Request interrupted") {
            self.push(StepKind::Note, "Interrupted".into(), None, None, at);
            return;
        }
        if self.info.first_prompt.is_none() {
            self.info.first_prompt = Some(t.to_owned());
        }
        self.push(StepKind::Prompt, truncate(t, MAX_TEXT), None, None, at);
    }
}

/// Markdown → plain text, enough for a one-glance summary: drops emphasis
/// and code markers and heading/bullet prefixes.
fn plain_text(md: &str) -> String {
    md.lines()
        .map(|l| l.trim_start().trim_start_matches(['#', '>']).trim_start())
        .map(|l| {
            l.strip_prefix("- ")
                .or_else(|| l.strip_prefix("* "))
                .unwrap_or(l)
        })
        .collect::<Vec<_>>()
        .join(" ")
        .replace("**", "")
        .replace("__", "")
        .replace('`', "")
        .trim()
        .to_owned()
}

/// "2026-11-16T09:35:45.728Z" → Unix milliseconds. UTC (`Z`) timestamps only,
/// which is what Claude Code writes.
pub fn parse_timestamp(s: &str) -> Option<u64> {
    let s = s.strip_suffix('Z')?;
    let (date, time) = s.split_once('T')?;
    let mut d = date.split('-').map(|p| p.parse::<i64>());
    let (y, m, day) = (d.next()?.ok()?, d.next()?.ok()?, d.next()?.ok()?);
    let (hms, frac) = time.split_once('.').unwrap_or((time, "0"));
    let mut t = hms.split(':').map(|p| p.parse::<i64>());
    let (hh, mm, ss) = (t.next()?.ok()?, t.next()?.ok()?, t.next()?.ok()?);
    let ms: i64 = format!("{:0<3}", &frac[..frac.len().min(3)]).parse().ok()?;
    // Days from civil (Howard Hinnant's algorithm).
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    let total = ((days * 24 + hh) * 60 + mm) * 60 + ss;
    u64::try_from(total * 1000 + ms).ok()
}

/// Transcripts (top-level `*.jsonl` under `projects/*/`) modified within
/// `within_ms`, newest first, with their modification time.
pub fn recent_transcripts(projects_dir: &Path, within_ms: u64) -> Vec<(PathBuf, u64)> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    let mut out = Vec::new();
    let Ok(projects) = std::fs::read_dir(projects_dir) else {
        return out;
    };
    for project in projects.flatten() {
        let Ok(files) = std::fs::read_dir(project.path()) else {
            continue;
        };
        for f in files.flatten() {
            let p = f.path();
            if p.extension().and_then(|e| e.to_str()) != Some("jsonl") {
                continue;
            }
            let Some(mtime) = f
                .metadata()
                .ok()
                .and_then(|m| m.modified().ok())
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map(|d| d.as_millis() as u64)
            else {
                continue;
            };
            if now.saturating_sub(mtime) <= within_ms {
                out.push((p, mtime));
            }
        }
    }
    out.sort_by_key(|f| std::cmp::Reverse(f.1));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn timestamps() {
        assert_eq!(parse_timestamp("1970-01-01T00:00:00.000Z"), Some(0));
        assert_eq!(
            parse_timestamp("2026-11-16T09:35:45.728Z"),
            Some(1_794_821_745_728)
        );
        assert_eq!(
            parse_timestamp("2000-02-29T12:00:00Z"),
            Some(951_825_600_000)
        );
        assert_eq!(parse_timestamp("2026-11-16 09:35"), None);
    }

    fn line(v: Value) -> String {
        v.to_string() + "\n"
    }

    #[test]
    fn steps_titles_and_incremental_reads() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("s1.jsonl");
        let mut f = std::fs::File::create(&path).unwrap();
        write!(
            f,
            "{}{}{}",
            line(serde_json::json!({"type": "user", "sessionId": "s1", "cwd": "/w/web", "timestamp": "2026-11-16T09:35:45.728Z",
                "message": {"role": "user", "content": "add a dark mode toggle"}})),
            line(serde_json::json!({"type": "user", "message": {"content": "<system-reminder>ignore</system-reminder>"}})),
            line(serde_json::json!({"type": "assistant", "message": {"content": [
                {"type": "text", "text": "Let me look."},
                {"type": "tool_use", "id": "t1", "name": "Read", "input": {"file_path": "/w/web/app.tsx"}}]}})),
        )
        .unwrap();
        // A partial line at the end must wait for its newline.
        write!(f, "{{\"type\": \"ai-title\", \"aiTi").unwrap();
        f.flush().unwrap();

        let mut t = Transcript::open(&path).unwrap();
        let kinds: Vec<_> = t
            .recent(10)
            .iter()
            .map(|s| (s.kind, s.text.clone(), s.status))
            .collect();
        assert_eq!(
            kinds,
            vec![
                (StepKind::Prompt, "add a dark mode toggle".into(), None),
                (StepKind::Text, "Let me look.".into(), None),
                (
                    StepKind::Tool,
                    "Reading app.tsx".into(),
                    Some(ToolStatus::Running)
                ),
            ]
        );
        assert_eq!(t.info.session_id.as_deref(), Some("s1"));
        assert_eq!(t.info.cwd.as_deref(), Some("/w/web"));
        assert_eq!(
            t.info.first_prompt.as_deref(),
            Some("add a dark mode toggle")
        );
        assert_eq!(t.recent(1)[0].seq, 2);
        assert_eq!(t.recent(10)[0].at, Some(1_794_821_745_728));

        write!(
            f,
            "tle\": \"Dark mode toggle\"}}\n{}{}",
            line(serde_json::json!({"type": "user", "message": {"content": [{"type": "tool_result", "tool_use_id": "t1", "is_error": true}]}})),
            line(serde_json::json!({"type": "custom-title", "customTitle": "Ship dark mode"})),
        )
        .unwrap();
        f.flush().unwrap();
        t.poll().unwrap();
        assert_eq!(t.info.ai_title.as_deref(), Some("Dark mode toggle"));
        assert_eq!(t.info.custom_title.as_deref(), Some("Ship dark mode"));
        assert_eq!(t.recent(1)[0].status, Some(ToolStatus::Error));
        assert_eq!(t.recent(10).len(), 3);
    }

    #[test]
    fn markdown_is_flattened() {
        assert_eq!(
            plain_text("## Done\n- **Mix** the `dough`\n- Let it rise"),
            "Done Mix the dough Let it rise"
        );
    }

    #[test]
    fn rejects_non_transcripts() {
        assert!(Transcript::open(Path::new("/etc/passwd")).is_err());
    }

    #[test]
    fn caps_steps() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("big.jsonl");
        let mut text = String::new();
        for i in 0..(MAX_STEPS + 50) {
            text += &line(
                serde_json::json!({"type": "assistant", "message": {"content": [
                {"type": "tool_use", "id": format!("t{i}"), "name": "Glob", "input": {}}]}}),
            );
        }
        text += &line(serde_json::json!({"type": "user", "message": {"content": [
            {"type": "tool_result", "tool_use_id": format!("t{}", MAX_STEPS + 49)}]}}));
        std::fs::write(&path, text).unwrap();
        let t = Transcript::open(&path).unwrap();
        assert_eq!(t.recent(1000).len(), MAX_STEPS);
        assert_eq!(t.recent(1)[0].status, Some(ToolStatus::Ok));
        assert_eq!(t.recent(1)[0].seq, MAX_STEPS + 49);
    }
}
