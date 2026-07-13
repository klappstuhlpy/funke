//! What `Settings::index_roots` *means*, in one place.
//!
//! Two providers search the user's files — `funke-files` by name, `funke-content` by what is
//! written inside them — and they have to agree about which folders that covers and which
//! paths are junk. Two copies of the rule would drift, and then the settings pane's account
//! of what is searched would be a fiction for one of them.
//!
//! Pure path logic, no filesystem walking beyond `is_dir` — it belongs in core for the same
//! reason [`Settings`](crate::Settings) does: it is what a preference means, not what a
//! provider does with it.

use std::path::PathBuf;

/// Directory names (lowercase) that are never worth searching: caches, build output, and
/// the recycle bin. A hit inside one of them is noise the user cannot act on.
const DIR_DENYLIST: &[&str] = &[
    "appdata",
    "node_modules",
    "target",
    "__pycache__",
    "venv",
    "$recycle.bin",
];

/// Settings roots → real roots: existing directories only, nested roots pruned (walking or
/// scoping to a parent already covers its children), and nothing configured means the user's
/// home directory — which is what an empty `index_roots` has always meant.
pub fn resolve_index_roots(configured: &[String]) -> Vec<PathBuf> {
    let existing: Vec<PathBuf> = configured
        .iter()
        .map(PathBuf::from)
        .filter(|path| path.is_dir())
        .collect();
    let roots = prune_nested(existing);
    if roots.is_empty() {
        dirs::home_dir().into_iter().collect()
    } else {
        roots
    }
}

/// Drop roots that live inside another root, so no subtree is covered twice.
fn prune_nested(mut roots: Vec<PathBuf>) -> Vec<PathBuf> {
    roots.sort();
    roots.dedup();
    let mut kept: Vec<PathBuf> = Vec::new();
    for root in roots {
        if !kept.iter().any(|parent| root.starts_with(parent)) {
            kept.push(root);
        }
    }
    kept
}

/// Is this a directory nobody wants results from? Expects a lowercase name.
pub fn denied_dir_name(name: &str) -> bool {
    name.starts_with('.') || DIR_DENYLIST.contains(&name)
}

/// Does this path pass through a denied directory? The walk skips those directories outright;
/// the two index backends that hand us finished paths (Everything, Windows Search) do not
/// know to, so their hits are filtered the same way. A whole-disk search is only a gift if it
/// isn't three screens of `node_modules`.
pub fn is_junk_path(path: &str) -> bool {
    path.split(['\\', '/'])
        .any(|segment| denied_dir_name(&segment.to_lowercase()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nested_and_duplicate_roots_are_pruned() {
        let pruned = prune_nested(vec![
            PathBuf::from(r"C:\Users\me\Documents"),
            PathBuf::from(r"C:\Users\me"),
            PathBuf::from(r"C:\Users\me"),
            PathBuf::from(r"D:\Media"),
        ]);
        assert_eq!(pruned, vec![PathBuf::from(r"C:\Users\me"), PathBuf::from(r"D:\Media")]);

        // Sibling with a shared name prefix is NOT nested.
        let pruned = prune_nested(vec![PathBuf::from(r"C:\data"), PathBuf::from(r"C:\database")]);
        assert_eq!(pruned.len(), 2);
    }

    #[test]
    fn missing_roots_fall_back_to_home() {
        let roots = resolve_index_roots(&["Z:\\does\\not\\exist".to_string()]);
        assert_eq!(roots, dirs::home_dir().into_iter().collect::<Vec<_>>());
    }

    #[test]
    fn junk_paths_are_recognized_with_either_separator() {
        assert!(is_junk_path(r"C:\dev\app\node_modules\left-pad\index.js"));
        assert!(is_junk_path(r"C:\Users\me\AppData\Local\cache.db"));
        assert!(is_junk_path(r"C:\Users\me\.git\config"));
        assert!(is_junk_path(r"C:\$Recycle.Bin\S-1-5-21\deleted.docx"));
        // Windows Search hands back `file:` URLs with forward slashes when it feels like it.
        assert!(is_junk_path("C:/dev/app/node_modules/left-pad/index.js"));

        assert!(!is_junk_path(r"C:\Users\me\Documents\report.xlsx"));
        assert!(!is_junk_path(r"C:\Windows\explorer.exe"));
    }

    #[test]
    fn junk_directories_are_denied() {
        assert!(denied_dir_name("node_modules"));
        assert!(denied_dir_name("appdata"));
        assert!(denied_dir_name(".git"));
        assert!(denied_dir_name("$recycle.bin"));
        assert!(!denied_dir_name("documents"));
        assert!(!denied_dir_name("projects"));
    }
}
