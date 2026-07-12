//! Agent sessions: `cc ` lists your Claude Code and Codex conversations newest-first, `cc <text>`
//! searches them by name, by the prompt you opened them with, or by project and branch, and Enter
//! resumes one in a terminal in the directory it ran in.
//!
//! **Two sources, one provider** — the shape `funke-files` uses for its walk and Everything. Both
//! tools answer the same question ("resume what I was working on"), so they share the row, the
//! ranking and the actions, and each row wears its own tool's icon to say which it is. A tool that
//! isn't installed contributes nothing, so having only one of the two costs nothing.
//!
//! A plugin rather than a compiled-in provider, deliberately. It needs none of the host seams the
//! built-in providers exist for (no focus capture, no masked prompts, no clipboard side-channel):
//! it reads files and spawns a process, which is exactly what an out-of-process plugin can do. It
//! also means the transcripts' *private* on-disk formats can be chased without cutting a launcher
//! release.
//!
//! `prefix_only`, for the same reason the snippets provider keeps its bodies out of global results:
//! an opening prompt is whatever you happened to type that day, and a global search must not
//! surface it.

mod claude;
mod codex;
mod session;
mod tool;

use std::path::Path;
use std::process::Command;

use funke_core::FuzzyMatcher;
use funke_plugin::proto::{PluginAction, PluginInfo, PluginItem};
use funke_plugin::sdk::{serve, Plugin};

use session::{ago, Session, SessionIndex};

/// Comfortably more than fits on screen, and under the registry's own cap of 50.
const MAX_ROWS: usize = 40;

/// The browse list (`cc ` with nothing typed) is ordered purely by recency, so its scores only
/// have to descend. Well clear of any fuzzy score, so a search never outranks it.
const BROWSE_BASE: i64 = 10_000;

/// How much a session's freshness may add to its match score: enough to break a tie between two
/// equally good matches in favour of the one you were just in, never enough to float a worse match
/// above a better one.
const MAX_RECENCY_BONUS: i64 = 48;

/// (row label, what it does) — index 0 runs on Enter, 1 on Shift+Enter, Tab lists them all.
type NamedAction = (&'static str, fn(&Session) -> Result<(), String>);

const ACTIONS: &[NamedAction] = &[
    ("Resume", resume),
    ("Open Folder", open_folder),
    ("Copy Resume Command", copy_command),
];

#[derive(Default)]
struct AgentSessions {
    index: SessionIndex,
}

impl Plugin for AgentSessions {
    fn info(&self) -> PluginInfo {
        PluginInfo {
            name: "Agent Sessions".into(),
            version: env!("CARGO_PKG_VERSION").into(),
            protocol: 0, // stamped by serve()
        }
    }

    fn query(&mut self, text: &str) -> Vec<PluginItem> {
        self.index.refresh();
        let sessions = self.index.sessions();

        let mut scored: Vec<(i64, &Session)> = match FuzzyMatcher::new(text.trim()) {
            // Nothing typed: the keyword alone is a browse view, newest first.
            None => sessions
                .iter()
                .take(MAX_ROWS)
                .enumerate()
                .map(|(rank, session)| (BROWSE_BASE - rank as i64, *session))
                .collect(),
            Some(matcher) => sessions
                .iter()
                .filter_map(|session| {
                    let score = session.fields().filter_map(|field| matcher.score(field)).max()?;
                    Some((score + recency_bonus(session), *session))
                })
                .collect(),
        };
        scored.sort_unstable_by(|(a, _), (b, _)| b.cmp(a));
        scored.truncate(MAX_ROWS);
        scored.into_iter().map(|(score, session)| row(score, session)).collect()
    }

    fn invoke(&mut self, item_id: &str, action_index: usize) -> Result<(), String> {
        let session = self.index.get(item_id).ok_or("that session is gone")?;
        let (_, run) = ACTIONS.get(action_index).ok_or("unknown action")?;
        run(session)
    }
}

fn row(score: i64, session: &Session) -> PluginItem {
    // "Claude Code · funke · main · 2h ago". The icon says which tool as well, but only the word
    // says it unambiguously — the two marks are small at row size.
    let mut subtitle = String::from(session.source.label());
    subtitle.push_str(" · ");
    subtitle.push_str(&session.project);
    if let Some(branch) = &session.branch {
        subtitle.push_str(" · ");
        subtitle.push_str(branch);
    }
    subtitle.push_str(" · ");
    subtitle.push_str(&ago(session.modified));

    PluginItem {
        id: session.id.clone(),
        title: session.title.clone(),
        subtitle: Some(subtitle),
        icon: Some(tool::icon(session.source).to_string()),
        score,
        actions: ACTIONS
            .iter()
            .map(|(label, _)| PluginAction {
                label: (*label).into(),
                confirm: false,
            })
            .collect(),
    }
}

/// Newer sessions edge ahead of older ones of equal match quality; past two days the bonus is
/// spent and only the match speaks.
fn recency_bonus(session: &Session) -> i64 {
    let hours = std::time::SystemTime::now()
        .duration_since(session.modified)
        .map(|elapsed| elapsed.as_secs() / 3_600)
        .unwrap_or(0) as i64;
    (MAX_RECENCY_BONUS - hours).clamp(0, MAX_RECENCY_BONUS)
}

/// Resume in Windows Terminal, falling back to a plain console if it isn't installed — both agents
/// are interactive, so they need a terminal to attach to, and the plugin (spawned with
/// `CREATE_NO_WINDOW`) has no window of its own to lend them.
fn resume(session: &Session) -> Result<(), String> {
    let source = session.source;
    let exe = tool::exe(source)
        .and_then(Path::to_str)
        .ok_or_else(|| format!("{} is not installed", source.label()))?;
    let cwd = as_str(&session.cwd)?;
    let args = tool::resume_args(source, &session.id);

    let started = Command::new("wt.exe").args(["-d", cwd, exe]).args(args).spawn().is_ok();
    if started {
        return Ok(());
    }
    Command::new("cmd.exe")
        .args(["/C", "start", source.label(), "/D", cwd, "cmd.exe", "/K", exe])
        .args(args)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("could not open a terminal: {e}"))
}

fn open_folder(session: &Session) -> Result<(), String> {
    Command::new("explorer.exe")
        .arg(as_str(&session.cwd)?)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("could not open the folder: {e}"))
}

/// For a terminal that is already open in the project.
fn copy_command(session: &Session) -> Result<(), String> {
    let command = format!(
        "{} {}",
        tool::command_name(session.source),
        tool::resume_args(session.source, &session.id).join(" ")
    );
    arboard::Clipboard::new()
        .and_then(|mut clipboard| clipboard.set_text(command))
        .map_err(|e| format!("copy failed: {e}"))
}

fn as_str(path: &Path) -> Result<&str, String> {
    path.to_str().ok_or_else(|| "that path is not valid UTF-8".to_string())
}

fn main() -> std::io::Result<()> {
    serve(AgentSessions::default())
}
