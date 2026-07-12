//! Finding each agent's binary, and giving each one a face.
//!
//! Both live here because a row's mark is bound up with the executable behind it. Where a binary
//! carries its own icon we take it, through the same shell API `funke-apps` uses for installed
//! programs — a real icon beats a copy of one. `claude.exe` does. `codex.exe` does **not**: it has
//! no icon resource, so the shell answers with Windows' generic console icon, which is the blank
//! page Codex rows used to wear. That one is drawn instead (see [`CODEX_MARK`]).
//!
//! Binaries are looked up on `PATH` and nowhere else — the plugin's contract is that the CLIs are
//! installed, and its manifest says so. A tool that isn't there still lists its sessions (a
//! transcript on disk is proof it once ran) and only refuses to *resume*, naming what is missing.
//! Both lookups are cached for the life of the process.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use funke_core::glyph_data_url;

use crate::session::Source;

/// Shown when the tool isn't installed, so its sessions still have a face.
const TERMINAL_GLYPH: &str =
    "<rect x='3' y='4' width='18' height='16' rx='2'/><path d='M7.5 9.5 10 12l-2.5 2.5'/><path d='M13 15h3.5'/>";

/// `--text-dim`, with `#` pre-encoded for the data URL — the same ink `funke_core::glyph_data_url`
/// uses, so a drawn mark sits with the house glyphs instead of shouting over them.
const INK: &str = "%23aaa295";

/// Codex's mark, drawn rather than taken.
///
/// `codex.exe` carries **no icon resource at all**, so asking the shell for its icon hands back
/// Windows' generic console icon — the blank page a row used to wear. There is nothing to extract,
/// so the mark is drawn here instead. (`claude.exe` does carry its own, which is why that one is
/// still taken from the binary: a real icon always beats a copy of one.)
const CODEX_MARK: &str = "<path d='M22.2819 9.8211a5.9847 5.9847 0 0 0-.5157-4.9108 6.0462 6.0462 0 0 0-6.5098-2.9A6.0651 6.0651 0 0 0 4.9807 4.1818a5.9847 5.9847 0 0 0-3.9977 2.9 6.0462 6.0462 0 0 0 .7427 7.0966 5.98 5.98 0 0 0 .511 4.9107 6.051 6.051 0 0 0 6.5146 2.9001A5.9847 5.9847 0 0 0 13.2599 24a6.0557 6.0557 0 0 0 5.7718-4.2058 5.9894 5.9894 0 0 0 3.9977-2.9001 6.0557 6.0557 0 0 0-.7475-7.0729zm-9.022 12.6081a4.4755 4.4755 0 0 1-2.8764-1.0408l.1419-.0804 4.7783-2.7582a.7948.7948 0 0 0 .3927-.6813v-6.7369l2.02 1.1686a.071.071 0 0 1 .038.052v5.5826a4.504 4.504 0 0 1-4.4945 4.4944zm-9.6607-4.1254a4.4708 4.4708 0 0 1-.5346-3.0137l.142.0852 4.783 2.7582a.7712.7712 0 0 0 .7806 0l5.8428-3.3685v2.3324a.0804.0804 0 0 1-.0332.0615L9.74 19.9502a4.4992 4.4992 0 0 1-6.1408-1.6464zM2.3408 7.8956a4.485 4.485 0 0 1 2.3655-1.9728V11.6a.7664.7664 0 0 0 .3879.6765l5.8144 3.3543-2.0201 1.1685a.0757.0757 0 0 1-.071 0l-4.8303-2.7865A4.504 4.504 0 0 1 2.3408 7.872zm16.5963 3.8558L13.1038 8.364 15.1192 7.2a.0757.0757 0 0 1 .071 0l4.8303 2.7913a4.4944 4.4944 0 0 1-.6765 8.1042v-5.6772a.79.79 0 0 0-.4067-.667zm2.0107-3.0231l-.142-.0852-4.7735-2.7818a.7759.7759 0 0 0-.7854 0L9.409 9.2297V6.8974a.0662.0662 0 0 1 .0284-.0615l4.8303-2.7866a4.4992 4.4992 0 0 1 6.6802 4.66zM8.3065 12.863l-2.02-1.1638a.0804.0804 0 0 1-.0379-.0568V6.0742a4.4992 4.4992 0 0 1 7.3757-3.4537l-.142.0805L8.704 5.459a.7948.7948 0 0 0-.3927.6813zm1.0976-2.3654l2.602-1.4998 2.6069 1.4998v2.9994l-2.5974 1.4997-2.6067-1.4997Z'/>";

/// A *filled* mark, where `funke_core::glyph_data_url` draws stroked outlines. Same ink, same
/// 24×24 box, same percent-encoding — a logo is a solid shape, not a line drawing.
fn drawn_mark(body: &str) -> String {
    format!(
        "data:image/svg+xml;utf8,<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 24 24' fill='{INK}'>{body}</svg>"
    )
    .replace(' ', "%20")
}

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

/// The tool's mark: taken from its binary where the binary has one, drawn where it doesn't.
pub fn icon(source: Source) -> &'static str {
    cache(source).icon.get_or_init(|| match source {
        // `claude.exe` embeds its own icon, so wear the real thing — the same shell API
        // `funke-apps` uses for installed programs. A house glyph if it isn't installed.
        Source::ClaudeCode => exe(source)
            .and_then(Path::to_str)
            .and_then(funke_shell::icon_data_url)
            .unwrap_or_else(|| glyph_data_url(TERMINAL_GLYPH)),
        // `codex.exe` embeds nothing, and asking the shell anyway returns Windows' generic
        // console icon — see [`CODEX_MARK`].
        Source::Codex => drawn_mark(CODEX_MARK),
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
