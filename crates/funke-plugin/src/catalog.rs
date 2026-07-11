//! The suggested-plugins catalog: a curated index of installable plugins, fetched over
//! HTTPS and unpacked into the plugins directory.
//!
//! **The trust story is the whole point of this module.** A plugin is an executable that
//! runs with the user's full rights (invariant: plugins are processes, not sandboxes), so
//! "install with one click" is only defensible if the bytes are pinned:
//!
//! - The index lives in the launcher's own repository ([`CATALOG_URL`]) and is curated by
//!   pull request — an entry gets in only if a human merged it.
//! - Every entry pins a download URL **and its SHA-256**. The archive is verified against
//!   that hash before a single file is written, so a plugin's release asset cannot be
//!   swapped out from under a catalog entry after review.
//! - Archive paths are validated before extraction (no absolute paths, no `..`, nothing
//!   outside the plugin's own folder), and the unpacked `plugin.json` must declare the id
//!   the catalog claimed. A failed install leaves nothing behind.
//!
//! What this does *not* do is make a reviewed plugin safe to run — it makes it the plugin
//! the reviewer saw. Anything beyond that is the user's trust decision, which is why the
//! settings pane says so out loud.

use std::io::Read;
use std::path::{Component, Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// The curated index. Raw file on the default branch — no server to run, no infrastructure
/// to trust beyond GitHub, and its history is the audit log.
pub const CATALOG_URL: &str = "https://raw.githubusercontent.com/klappstuhlpy/funke/main/plugins.json";

const FETCH_TIMEOUT: Duration = Duration::from_secs(10);
/// A plugin is a small executable; anything this size is a mistake or an attack.
const MAX_DOWNLOAD: usize = 64 * 1024 * 1024;

/// One installable plugin, as the catalog describes it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CatalogEntry {
    /// Must match the `id` in the packaged `plugin.json`, and names the install folder.
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub homepage: String,
    #[serde(default)]
    pub version: String,
    /// Scope keyword, shown so the user knows what to type after installing.
    #[serde(default)]
    pub prefix: Option<String>,
    /// Zip archive: either `<id>/…` or flat files (both unpack into `<plugins>/<id>/`).
    pub url: String,
    /// Hex SHA-256 of the archive. The entry *is* the hash — see the module docs.
    pub sha256: String,
}

#[derive(Debug, Deserialize)]
struct Catalog {
    #[serde(default)]
    plugins: Vec<CatalogEntry>,
}

/// Fetch and parse the index. Network failures are returned, never panicked on — a launcher
/// that can't reach GitHub still launches things (invariant 4).
pub fn fetch(url: &str) -> Result<Vec<CatalogEntry>, String> {
    let body = ureq::AgentBuilder::new()
        .timeout(FETCH_TIMEOUT)
        .build()
        .get(url)
        .call()
        .map_err(|e| format!("could not reach the plugin catalog: {e}"))?
        .into_string()
        .map_err(|e| format!("could not read the plugin catalog: {e}"))?;
    let catalog: Catalog = serde_json::from_str(&body).map_err(|e| format!("malformed plugin catalog: {e}"))?;
    Ok(catalog.plugins)
}

/// Download, verify, unpack. Returns the folder the plugin now lives in.
///
/// The plugin's child process must already be stopped (see `PluginManager::remove`) —
/// Windows will not let us replace an executable that is running.
pub fn install(entry: &CatalogEntry, plugins_dir: &Path) -> Result<PathBuf, String> {
    let archive = download(&entry.url)?;
    verify(&archive, &entry.sha256)?;
    unpack(&archive, entry, plugins_dir)
}

/// Delete an installed plugin's folder. Its process must be stopped first, or Windows keeps
/// the running exe locked; we retry briefly because the kill is asynchronous.
pub fn remove(id: &str, plugins_dir: &Path) -> Result<(), String> {
    let dir = plugins_dir.join(id);
    if !dir.exists() {
        return Ok(());
    }
    let mut last = String::new();
    for attempt in 0..10 {
        match std::fs::remove_dir_all(&dir) {
            Ok(()) => return Ok(()),
            Err(e) => {
                last = e.to_string();
                std::thread::sleep(Duration::from_millis(50 * (attempt + 1)));
            }
        }
    }
    Err(format!("could not remove {}: {last}", dir.display()))
}

fn download(url: &str) -> Result<Vec<u8>, String> {
    if !url.starts_with("https://") {
        return Err("catalog downloads must be https".into());
    }
    let response = ureq::AgentBuilder::new()
        .timeout(FETCH_TIMEOUT)
        .build()
        .get(url)
        .call()
        .map_err(|e| format!("download failed: {e}"))?;
    let mut bytes = Vec::new();
    response
        .into_reader()
        .take(MAX_DOWNLOAD as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| format!("download failed: {e}"))?;
    if bytes.len() > MAX_DOWNLOAD {
        return Err("download is implausibly large for a plugin".into());
    }
    Ok(bytes)
}

fn verify(bytes: &[u8], expected: &str) -> Result<(), String> {
    let actual = format!("{:x}", Sha256::digest(bytes));
    if !actual.eq_ignore_ascii_case(expected.trim()) {
        return Err(format!(
            "checksum mismatch — the download does not match the catalog (expected {expected}, got {actual})"
        ));
    }
    Ok(())
}

/// Unpack into `<plugins_dir>/<id>`, via a staging folder so a half-written plugin can
/// never end up loaded: the live folder is only touched once every file is on disk and the
/// manifest has been checked.
fn unpack(archive: &[u8], entry: &CatalogEntry, plugins_dir: &Path) -> Result<PathBuf, String> {
    let target = plugins_dir.join(&entry.id);
    let staging = plugins_dir.join(format!(".{}.staging", entry.id));
    std::fs::create_dir_all(plugins_dir).map_err(|e| e.to_string())?;
    std::fs::remove_dir_all(&staging).ok();

    let result = extract(archive, &entry.id, &staging).and_then(|()| check_manifest(&staging, &entry.id));
    if let Err(e) = result {
        std::fs::remove_dir_all(&staging).ok();
        return Err(e);
    }

    std::fs::remove_dir_all(&target).ok();
    std::fs::rename(&staging, &target).map_err(|e| {
        std::fs::remove_dir_all(&staging).ok();
        format!("could not install into {}: {e}", target.display())
    })?;
    Ok(target)
}

fn extract(archive: &[u8], id: &str, staging: &Path) -> Result<(), String> {
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(archive)).map_err(|e| format!("not a zip archive: {e}"))?;
    for index in 0..zip.len() {
        let mut file = zip.by_index(index).map_err(|e| e.to_string())?;
        let Some(relative) = safe_path(file.name(), id)? else {
            continue; // the archive's own `<id>/` root, or an empty path
        };
        let path = staging.join(relative);
        if file.is_dir() {
            std::fs::create_dir_all(&path).map_err(|e| e.to_string())?;
            continue;
        }
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let mut out = std::fs::File::create(&path).map_err(|e| e.to_string())?;
        std::io::copy(&mut file, &mut out).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Zip-slip guard. An archive entry may only land *inside* the plugin's own folder: no
/// absolute paths, no drive letters, no `..`, nothing above the root. A leading `<id>/` is
/// stripped, so both packaging shapes (folder or flat) install the same way.
fn safe_path(name: &str, id: &str) -> Result<Option<PathBuf>, String> {
    let raw = Path::new(name.trim_end_matches('/'));
    let mut parts: Vec<&std::ffi::OsStr> = Vec::new();
    for component in raw.components() {
        match component {
            Component::Normal(part) => parts.push(part),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(format!("refusing archive entry that escapes its folder: {name}"))
            }
        }
    }
    if parts.first().is_some_and(|first| first.eq_ignore_ascii_case(id)) {
        parts.remove(0);
    }
    if parts.is_empty() {
        return Ok(None);
    }
    Ok(Some(parts.iter().collect()))
}

/// The unpacked plugin must be the one the catalog promised — otherwise an entry could
/// quietly install itself over a different plugin's id.
fn check_manifest(staging: &Path, id: &str) -> Result<(), String> {
    let manifest = staging.join("plugin.json");
    let raw =
        std::fs::read_to_string(&manifest).map_err(|_| "the archive has no plugin.json at its root".to_string())?;
    let parsed: crate::host::Manifest = serde_json::from_str(&raw).map_err(|e| format!("bad plugin.json: {e}"))?;
    if parsed.id != id {
        return Err(format!(
            "the archive declares plugin id `{}`, but the catalog offered `{id}`",
            parsed.id
        ));
    }
    if !staging.join(&parsed.entry).is_file() {
        return Err(format!("the archive is missing its entry point `{}`", parsed.entry));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn zipped(files: &[(&str, &[u8])]) -> Vec<u8> {
        let mut buffer = Vec::new();
        {
            let mut writer = zip::ZipWriter::new(std::io::Cursor::new(&mut buffer));
            for (name, contents) in files {
                writer
                    .start_file::<_, ()>(*name, zip::write::SimpleFileOptions::default())
                    .unwrap();
                writer.write_all(contents).unwrap();
            }
            writer.finish().unwrap();
        }
        buffer
    }

    fn entry(id: &str, archive: &[u8]) -> CatalogEntry {
        CatalogEntry {
            id: id.into(),
            name: id.into(),
            description: String::new(),
            author: String::new(),
            homepage: String::new(),
            version: "1.0.0".into(),
            prefix: None,
            url: format!("https://example.invalid/{id}.zip"),
            sha256: format!("{:x}", Sha256::digest(archive)),
        }
    }

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("funke-catalog-{name}"));
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_well_formed_archive_installs() {
        let archive = zipped(&[
            ("demo/plugin.json", br#"{"id":"demo","name":"Demo","entry":"demo.exe"}"#),
            ("demo/demo.exe", b"MZ"),
        ]);
        let dir = scratch("install");
        let installed = install_from_bytes(&archive, &entry("demo", &archive), &dir).unwrap();
        assert_eq!(installed, dir.join("demo"));
        assert!(installed.join("plugin.json").is_file());
        assert!(installed.join("demo.exe").is_file(), "the leading `demo/` is stripped");
        assert!(!dir.join(".demo.staging").exists(), "staging is cleaned up");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_flat_archive_installs_the_same_way() {
        let archive = zipped(&[
            ("plugin.json", br#"{"id":"flat","name":"Flat","entry":"flat.exe"}"#),
            ("flat.exe", b"MZ"),
        ]);
        let dir = scratch("flat");
        let installed = install_from_bytes(&archive, &entry("flat", &archive), &dir).unwrap();
        assert!(installed.join("flat.exe").is_file());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_tampered_archive_is_refused_before_anything_is_written() {
        let archive = zipped(&[("plugin.json", br#"{"id":"x","name":"X","entry":"x.exe"}"#)]);
        let mut listing = entry("x", &archive);
        listing.sha256 = "0".repeat(64);
        let error = verify(&archive, &listing.sha256).unwrap_err();
        assert!(error.contains("checksum mismatch"), "{error}");
    }

    #[test]
    fn an_archive_that_lies_about_its_id_is_refused_and_leaves_nothing_behind() {
        let archive = zipped(&[
            ("plugin.json", br#"{"id":"evil","name":"Evil","entry":"evil.exe"}"#),
            ("evil.exe", b"MZ"),
        ]);
        let dir = scratch("liar");
        let error = install_from_bytes(&archive, &entry("innocent", &archive), &dir).unwrap_err();
        assert!(error.contains("declares plugin id"), "{error}");
        assert!(!dir.join("innocent").exists());
        assert!(!dir.join(".innocent.staging").exists());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn zip_slip_paths_are_refused() {
        for hostile in [
            "../evil.exe",
            "demo/../../evil.exe",
            "/etc/passwd",
            "C:\\windows\\evil.exe",
        ] {
            assert!(
                safe_path(hostile, "demo").is_err(),
                "{hostile} should never be extracted"
            );
        }
        assert_eq!(safe_path("demo/", "demo").unwrap(), None, "the archive root is skipped");
        assert_eq!(
            safe_path("demo/sub/file.txt", "demo").unwrap().unwrap(),
            PathBuf::from("sub").join("file.txt")
        );
    }

    #[test]
    fn an_archive_missing_its_entry_point_is_refused() {
        let archive = zipped(&[("plugin.json", br#"{"id":"gone","name":"Gone","entry":"gone.exe"}"#)]);
        let dir = scratch("no-entry");
        let error = install_from_bytes(&archive, &entry("gone", &archive), &dir).unwrap_err();
        assert!(error.contains("missing its entry point"), "{error}");
        std::fs::remove_dir_all(&dir).ok();
    }

    /// `install` minus the download — the network half is not unit-testable, the rest is.
    fn install_from_bytes(archive: &[u8], entry: &CatalogEntry, dir: &Path) -> Result<PathBuf, String> {
        verify(archive, &entry.sha256)?;
        unpack(archive, entry, dir)
    }
}
