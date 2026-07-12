//! Finding each agent's binary — and borrowing its face for the row.
//!
//! One lookup serves both jobs, which is why they live together: the executable we resume a
//! session with is also the executable whose embedded icon marks the row as that tool's. So a row
//! wears the real Claude Code or Codex mark (via the same shell API `funke-apps` uses for
//! installed programs) rather than a hand-drawn imitation of somebody's logo, and it can only
//! wear it when the tool is actually there to be launched.
//!
//! Both are looked up on `PATH` and nowhere else — the plugin's contract is that the two CLIs are
//! installed (their manifest says so). A tool that isn't there simply has no icon of its own and
//! refuses to resume, saying which one is missing; its sessions still list, because a transcript
//! on disk is proof the tool once ran. Both lookups are cached for the life of the process.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use funke_core::glyph_data_url;

use crate::session::Source;

/// Shown when the tool isn't installed, so its sessions still have a face.
const TERMINAL_GLYPH: &str =
    "<rect x='3' y='4' width='18' height='16' rx='2'/><path d='M7.5 9.5 10 12l-2.5 2.5'/><path d='M13 15h3.5'/>";

#[derive(Default)]
struct Cache {
    exe: OnceLock<Option<PathBuf>>,
    icon: OnceLock<String>,
}

static CLAUDE_CODE: Cache = Cache {
    exe: OnceLock::new(),
    icon: OnceLock::new(),
};
static CODEX: Cache = Cache {
    exe: OnceLock::new(),
    icon: OnceLock::new(),
};

fn cache(source: Source) -> &'static Cache {
    match source {
        Source::ClaudeCode => &CLAUDE_CODE,
        Source::Codex => &CODEX,
    }
}

/// The tool's executable, or `None` if it isn't installed.
pub fn exe(source: Source) -> Option<&'static Path> {
    cache(source).exe.get_or_init(|| locate(source)).as_deref()
}

/// The tool's own icon, falling back to a house glyph when it isn't installed to take one from.
pub fn icon(source: Source) -> &'static str {
    cache(source).icon.get_or_init(|| {
        exe(source)
            .and_then(Path::to_str)
            .and_then(funke_shell::icon_data_url)
            .unwrap_or_else(|| glyph_data_url(TERMINAL_GLYPH))
    })
}

/// What the tool is called on a command line — for the row's "copy the command" action, which is
/// for pasting into a terminal, not for spawning.
pub fn command_name(source: Source) -> &'static str {
    match source {
        Source::ClaudeCode => "claude",
        Source::Codex => "codex",
    }
}

/// The two tools spell it differently: `claude --resume <id>` against `codex resume <id>`.
pub fn resume_args(source: Source, id: &str) -> [&str; 2] {
    match source {
        Source::ClaudeCode => ["--resume", id],
        Source::Codex => ["resume", id],
    }
}

fn locate(source: Source) -> Option<PathBuf> {
    let file = match source {
        Source::ClaudeCode => "claude.exe",
        Source::Codex => "codex.exe",
    };
    std::env::split_paths(&std::env::var_os("PATH")?)
        .map(|dir| dir.join(file))
        .find(|candidate| candidate.is_file())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_tool_spells_resume_its_own_way() {
        assert_eq!(resume_args(Source::ClaudeCode, "abc"), ["--resume", "abc"]);
        assert_eq!(resume_args(Source::Codex, "abc"), ["resume", "abc"]);
    }

    #[test]
    fn a_tool_that_is_not_installed_still_has_a_face() {
        // Whether either is installed depends on the machine, so assert the invariant that holds
        // either way: there is always an icon, and it is always a data URL.
        for source in Source::ALL {
            assert!(icon(source).starts_with("data:image/"), "{}", source.label());
        }
    }
}
