//! Reading Claude Code's session transcripts.
//!
//! A session is `%USERPROFILE%/.claude/projects/<encoded-cwd>/<uuid>.jsonl`, one JSON object per
//! line, and the file's own name is the id `claude --resume` takes. A row is assembled from the
//! head of the file, out of two of those lines:
//!
//! ```json
//! {"type":"ai-title","aiTitle":"Add a Claude Code plugin"}
//! {"type":"user","message":{"role":"user","content":"add a plugin"},"origin":{"kind":"human"},
//!  "isSidechain":false,"cwd":"C:\\Users\\me\\funke","gitBranch":"main","version":"2.1.207"}
//! ```
//!
//! Claude Code names its own conversations (`ai-title`), and that name makes a better row than
//! the raw opening prompt — which is whatever the user happened to be mid-thought about. So the
//! title is the `aiTitle` when there is one and the opening prompt when there isn't (older
//! sessions have none), and the prompt stays searchable either way: you may well remember what
//! you *asked* rather than what the session came to be called. Codex, having no such name, is
//! titled by its prompt always — see [`crate::codex`].
//!
//! This is Claude Code's private format, not a published one — note the `version` it stamps on
//! every line. So every field here is optional, a line that doesn't fit is skipped, and a session
//! we cannot make sense of is simply not listed. A format change must cost the user rows, never a
//! crash.
//!
//! The folder name is a *lossy* encoding of the path (every separator becomes a dash), so it
//! cannot be inverted — the `cwd` field is the only trustworthy source of the directory, and a
//! session without one is dropped: resuming in the wrong directory would not find it.

use std::path::{Path, PathBuf};

use serde::Deserialize;
use serde_json::Value;

use crate::session::{collapse, head_lines, truncate, Head, MAX_TITLE_CHARS};

/// One transcript line, as much of it as we care about.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Line {
    #[serde(rename = "type", default)]
    kind: Option<String>,
    #[serde(default)]
    ai_title: Option<String>,
    #[serde(default)]
    message: Option<Message>,
    #[serde(default)]
    cwd: Option<String>,
    #[serde(default)]
    git_branch: Option<String>,
    #[serde(default)]
    is_sidechain: Option<bool>,
    #[serde(default)]
    is_meta: Option<bool>,
    /// The preamble Claude Code writes when a conversation is continued past its context window.
    /// It is a `user` line, but the user did not type it.
    #[serde(default)]
    is_compact_summary: Option<bool>,
    #[serde(default)]
    origin: Option<Origin>,
}

#[derive(Deserialize)]
struct Message {
    #[serde(default)]
    content: Option<Value>,
}

#[derive(Deserialize)]
struct Origin {
    #[serde(default)]
    kind: Option<String>,
}

/// Slash commands and injected context arrive as `type:"user"` lines too. They are machinery, not
/// something the user would recognize as the start of a conversation.
const NOT_A_PROMPT: &[&str] = &[
    "<command-name>",
    "<command-message>",
    "<local-command-stdout>",
    "<user-memory-input>",
];

pub fn read(path: &Path) -> Option<Head> {
    let id = path.file_stem()?.to_str()?.to_string();
    parse_head(head_lines(path)?, id)
}

/// Assemble a row from the head of a transcript. `None` if it yields no title or no directory —
/// either way there is nothing we could usefully offer to resume.
fn parse_head(lines: impl Iterator<Item = String>, id: String) -> Option<Head> {
    let mut ai_title = None;
    let mut prompt = None;
    let mut cwd = None;
    let mut branch = None;

    for line in lines {
        let Ok(line) = serde_json::from_str::<Line>(&line) else {
            continue; // not a shape we know — the format moved, or the line was truncated
        };
        match line.kind.as_deref() {
            Some("ai-title") if ai_title.is_none() => {
                ai_title = line.ai_title.as_deref().map(collapse).filter(|t| !t.is_empty());
            }
            Some("user") if prompt.is_none() => prompt = human_prompt(&line),
            _ => {}
        }
        // Most lines are stamped with the directory, so take it from whichever comes first — a
        // session compacted and then closed has no prompt of its own to carry it.
        if cwd.is_none() {
            if let Some(dir) = line.cwd.as_deref().filter(|dir| !dir.is_empty()) {
                cwd = Some(PathBuf::from(dir));
                branch = line.git_branch.as_deref().filter(|b| !b.is_empty()).map(str::to_owned);
            }
        }
        if ai_title.is_some() && prompt.is_some() && cwd.is_some() {
            break;
        }
    }

    // The name Claude Code gave it, or failing that what the user opened with.
    let title = truncate(&ai_title.or_else(|| prompt.clone())?, MAX_TITLE_CHARS);
    Some(Head {
        id,
        prompt: prompt.filter(|p| *p != title),
        cwd: cwd?, // no cwd, no resume — see the module note
        branch,
        title,
    })
}

/// The opening prompt, if this `user` line is one. Tool results, subagent turns, injected context,
/// slash-command stubs and the post-compaction preamble all arrive as `user` lines; none of them
/// is somebody typing.
fn human_prompt(line: &Line) -> Option<String> {
    if line.is_sidechain == Some(true) || line.is_meta == Some(true) || line.is_compact_summary == Some(true) {
        return None;
    }
    // `origin` is newer than the oldest transcripts: demand "human" when it is there, and lean on
    // the content check when it isn't.
    if line.origin.as_ref().is_some_and(|o| o.kind.as_deref() != Some("human")) {
        return None;
    }
    let text = collapse(&text_of(line.message.as_ref()?.content.as_ref()?)?);
    if text.is_empty() || NOT_A_PROMPT.iter().any(|tag| text.starts_with(tag)) {
        return None;
    }
    Some(text)
}

/// A prompt is a bare string; tool results and multimodal turns are content blocks. Taking only
/// the first `text` block is what rejects a tool result (it has none) without naming it.
fn text_of(content: &Value) -> Option<String> {
    match content {
        Value::String(text) => Some(text.clone()),
        Value::Array(blocks) => blocks.iter().find_map(|block| {
            if block.get("type").and_then(Value::as_str) == Some("text") {
                block.get("text").and_then(Value::as_str).map(str::to_owned)
            } else {
                None
            }
        }),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::BufRead;

    const PROMPT: &str = r#"{"type":"user","message":{"role":"user","content":"add a plugin"},"origin":{"kind":"human"},"isSidechain":false,"cwd":"C:\\Users\\me\\funke","gitBranch":"main"}"#;

    fn head_of(lines: &[&str]) -> Option<Head> {
        parse_head(
            lines.join("\n").as_bytes().lines().map_while(Result::ok),
            "session-uuid".to_string(),
        )
    }

    #[test]
    fn takes_the_opening_prompt_and_the_directory_it_ran_in() {
        let head = head_of(&[r#"{"type":"mode","mode":"normal"}"#, PROMPT]).unwrap();
        assert_eq!(head.id, "session-uuid"); // the file's name — what `claude --resume` takes
        assert_eq!(head.title, "add a plugin");
        assert_eq!(head.prompt, None); // it *is* the title — don't search it twice
        assert_eq!(head.cwd, PathBuf::from(r"C:\Users\me\funke"));
        assert_eq!(head.branch.as_deref(), Some("main"));
    }

    #[test]
    fn the_conversations_own_name_wins_over_the_prompt_that_opened_it() {
        let head = head_of(&[r#"{"type":"ai-title","aiTitle":"Add a Claude Code plugin"}"#, PROMPT]).unwrap();
        assert_eq!(head.title, "Add a Claude Code plugin");
        // …but you may remember what you asked rather than what it came to be called.
        assert_eq!(head.prompt.as_deref(), Some("add a plugin"));
    }

    #[test]
    fn a_compacted_session_is_not_titled_by_its_preamble() {
        // Continuing past the context window writes a `user` line the user never typed. The real
        // prompt is the one they went on to send.
        let head = head_of(&[
            r#"{"type":"user","message":{"content":"This session is being continued from a previous conversation…"},"isCompactSummary":true,"cwd":"C:\\Users\\me\\funke"}"#,
            r#"{"type":"user","message":{"content":"now ship it"},"origin":{"kind":"human"},"cwd":"C:\\Users\\me\\funke"}"#,
        ])
        .unwrap();
        assert_eq!(head.title, "now ship it");
    }

    #[test]
    fn a_compacted_session_that_was_never_touched_again_still_has_its_name() {
        // No prompt of its own, but a title and a directory — enough to resume.
        let head = head_of(&[
            r#"{"type":"ai-title","aiTitle":"Rework the settings pane"}"#,
            r#"{"type":"user","message":{"content":"This session is being continued…"},"isCompactSummary":true,"cwd":"C:\\Users\\me\\funke","gitBranch":"main"}"#,
        ])
        .unwrap();
        assert_eq!(head.title, "Rework the settings pane");
        assert_eq!(head.prompt, None);
        assert_eq!(head.branch.as_deref(), Some("main"));
    }

    #[test]
    fn skips_lines_that_are_not_somebody_typing() {
        // A tool result, a subagent's turn, an injected reminder, and a slash command all arrive
        // as `type:"user"` — none of them opened the conversation.
        let head = head_of(&[
            r#"{"type":"user","message":{"content":[{"type":"tool_result","content":"ok"}]},"cwd":"C:\\a"}"#,
            r#"{"type":"user","message":{"content":"sidechain"},"isSidechain":true,"cwd":"C:\\a"}"#,
            r#"{"type":"user","message":{"content":"reminder"},"isMeta":true,"cwd":"C:\\a"}"#,
            r#"{"type":"user","message":{"content":"<command-name>/clear</command-name>"},"cwd":"C:\\a"}"#,
            r#"{"type":"assistant","message":{"content":"hello"},"cwd":"C:\\a"}"#,
            PROMPT,
        ])
        .unwrap();
        assert_eq!(head.title, "add a plugin");
    }

    #[test]
    fn reads_a_prompt_out_of_content_blocks() {
        let head = head_of(&[
            r#"{"type":"user","message":{"content":[{"type":"text","text":"look at this"}]},"cwd":"C:\\a"}"#,
        ])
        .unwrap();
        assert_eq!(head.title, "look at this");
    }

    #[test]
    fn a_session_that_never_says_where_it_ran_is_unusable() {
        // Without a cwd we would resume in the wrong directory and find nothing, and the folder
        // name is a lossy encoding of the path — it cannot be inverted.
        assert!(head_of(&[r#"{"type":"user","message":{"content":"hi"},"origin":{"kind":"human"}}"#]).is_none());
    }

    #[test]
    fn a_format_we_no_longer_understand_costs_rows_not_a_crash() {
        assert!(head_of(&["not json at all", "", r#"{"type":"user"}"#, "{}"]).is_none());
    }
}
