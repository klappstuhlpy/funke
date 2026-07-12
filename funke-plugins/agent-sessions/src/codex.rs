//! Reading Codex's rollout transcripts.
//!
//! A session is `%USERPROFILE%/.codex/sessions/<yyyy>/<mm>/<dd>/rollout-<ts>-<uuid>.jsonl`. Each
//! line is a `RolloutLine`: a timestamp with a `RolloutItem` flattened into it, the item tagged
//! `type` with its body under `payload`. Two of those lines make a row:
//!
//! ```json
//! {"timestamp":"…","type":"session_meta","payload":{"id":"019f556f-…","cwd":"C:\\…\\funke",
//!   "originator":"Codex Desktop","git":{"branch":"main"}}}
//! {"timestamp":"…","type":"event_msg","payload":{"type":"user_message","message":"add a plugin"}}
//! ```
//!
//! Unlike Claude Code, Codex does not name its conversations — its own session lister derives a
//! preview "from the first UserMessage" too. So the title is always the opening prompt.
//!
//! The `session_meta` payload is where the directory comes from, and its `id` is what `codex
//! resume` takes. Note the prompt is an `event_msg`, *not* one of the `response_item` lines that
//! precede it: those carry the instructions Codex sends the model, which the user never typed.

use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::session::{collapse, head_lines, truncate, Head, MAX_TITLE_CHARS};

/// One rollout line. `RolloutItem` is `#[serde(tag = "type", content = "payload")]`, flattened
/// into the line — so the discriminator and the body land here side by side.
#[derive(Deserialize)]
struct Line {
    #[serde(rename = "type", default)]
    kind: Option<String>,
    #[serde(default)]
    payload: Option<Payload>,
}

/// The two payloads we care about, in one shape: a `session_meta` carries the directory, an
/// `event_msg` carries its own inner `type` and the message.
#[derive(Deserialize)]
struct Payload {
    // session_meta
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    cwd: Option<String>,
    #[serde(default)]
    git: Option<Git>,
    // event_msg
    #[serde(rename = "type", default)]
    kind: Option<String>,
    #[serde(default)]
    message: Option<String>,
}

#[derive(Deserialize)]
struct Git {
    #[serde(default)]
    branch: Option<String>,
}

pub fn read(path: &Path) -> Option<Head> {
    let mut id = None;
    let mut cwd = None;
    let mut branch = None;
    let mut prompt = None;

    for line in head_lines(path)? {
        let Ok(line) = serde_json::from_str::<Line>(&line) else {
            continue; // not a shape we know — the format moved, or the line was truncated
        };
        let Some(payload) = line.payload else { continue };
        match line.kind.as_deref() {
            Some("session_meta") if id.is_none() => {
                id = payload.id.filter(|id| !id.is_empty());
                cwd = payload.cwd.filter(|dir| !dir.is_empty()).map(PathBuf::from);
                // `git` is present but empty outside a repository — hence two layers of Option.
                branch = payload.git.and_then(|git| git.branch).filter(|b| !b.is_empty());
            }
            Some("event_msg") if prompt.is_none() && payload.kind.as_deref() == Some("user_message") => {
                prompt = payload.message.as_deref().map(collapse).filter(|p| !p.is_empty());
            }
            _ => {}
        }
        if id.is_some() && cwd.is_some() && prompt.is_some() {
            break;
        }
    }

    // Codex has no name of its own for a session, so the prompt is the title. Without it, or
    // without the directory to resume in, there is no row worth offering.
    let title = truncate(&prompt?, MAX_TITLE_CHARS);
    Some(Head {
        id: id?,
        prompt: None, // it *is* the title — don't search it twice
        cwd: cwd?,
        branch,
        title,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    const META: &str = r#"{"timestamp":"2026-07-12T08:27:11.146Z","type":"session_meta","payload":{"session_id":"019f556f","id":"019f556f-cb56-7b03","timestamp":"…","cwd":"C:\\Users\\me\\funke","originator":"Codex Desktop","cli_version":"0.144.0","git":{"branch":"main","commit_hash":"abc"}}}"#;
    const PROMPT: &str = r#"{"timestamp":"2026-07-12T08:27:20.000Z","type":"event_msg","payload":{"type":"user_message","message":"Please give me an short overview\n","images":null}}"#;

    /// The reader takes a path, so a fixture has to land on disk.
    fn head_of(lines: &[&str]) -> Option<Head> {
        let path = std::env::temp_dir().join(format!("funke-codex-{:?}.jsonl", std::thread::current().id()));
        let mut file = std::fs::File::create(&path).unwrap();
        writeln!(file, "{}", lines.join("\n")).unwrap();
        drop(file);
        let head = read(&path);
        let _ = std::fs::remove_file(&path);
        head
    }

    #[test]
    fn takes_the_id_the_directory_and_the_prompt() {
        let head = head_of(&[META, PROMPT]).unwrap();
        assert_eq!(head.id, "019f556f-cb56-7b03"); // what `codex resume` takes
        assert_eq!(head.cwd, PathBuf::from(r"C:\Users\me\funke"));
        assert_eq!(head.branch.as_deref(), Some("main"));
        assert_eq!(head.title, "Please give me an short overview");
        assert_eq!(head.prompt, None); // the prompt *is* the title
    }

    #[test]
    fn the_prompt_is_the_event_msg_not_the_instructions_sent_to_the_model() {
        // Codex writes several `response_item` lines with `role:"user"` before the first typed
        // word — they are the instructions it sends the model, and titling a row with them would
        // give every session the same name.
        let head = head_of(&[
            META,
            r#"{"timestamp":"…","type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"<user_instructions>be terse</user_instructions>"}]}}"#,
            r#"{"timestamp":"…","type":"turn_context","payload":{"cwd":"C:\\elsewhere","model":"gpt-5"}}"#,
            PROMPT,
        ])
        .unwrap();
        assert_eq!(head.title, "Please give me an short overview");
        // …and `turn_context` also carries a cwd. The session's own is the one that counts.
        assert_eq!(head.cwd, PathBuf::from(r"C:\Users\me\funke"));
    }

    #[test]
    fn a_session_outside_a_repository_simply_has_no_branch() {
        // Codex writes `"git":{}` when there is no repo — an empty object, not a missing key.
        let head = head_of(&[
            r#"{"timestamp":"…","type":"session_meta","payload":{"id":"abc","cwd":"C:\\tmp","git":{}}}"#,
            PROMPT,
        ])
        .unwrap();
        assert_eq!(head.branch, None);
    }

    #[test]
    fn a_rollout_with_no_typed_word_is_not_a_row() {
        assert!(head_of(&[META]).is_none());
    }

    #[test]
    fn a_format_we_no_longer_understand_costs_rows_not_a_crash() {
        assert!(head_of(&["not json at all", "", "{}", r#"{"type":"session_meta"}"#]).is_none());
    }
}
