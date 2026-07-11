use std::path::Path;

fn main() {
    stage_brand_icon();
    tauri_build::build();
}

/// The settings window's brand mark is the same PNG the bundle and tray use, so `icons/` is
/// the only copy in git. A webview can reach nothing outside `frontendDist` (`ui/`), so the
/// icon is staged in at build time rather than committed twice — and only when the bytes
/// actually differ, since rewriting it every build would churn the mtime and recompile the
/// crate each time.
fn stage_brand_icon() {
    let staged = Path::new("ui/icon.png");
    println!("cargo:rerun-if-changed=icons/icon.png");
    // …and on the staged copy itself, or the staging is not self-healing: with only the source
    // watched, a `ui/icon.png` that goes missing (a clean checkout of an ignored file, a stray
    // delete) never comes back — cargo sees no reason to re-run this script, and the settings
    // window quietly renders a broken image where the brand mark should be. A missing file
    // counts as changed, so naming the output here is what brings it back.
    println!("cargo:rerun-if-changed=ui/icon.png");
    let bytes = std::fs::read(Path::new("icons/icon.png")).expect("icons/icon.png is the app mark");
    if std::fs::read(staged).ok().as_deref() != Some(&bytes) {
        std::fs::write(staged, &bytes).expect("failed to stage the app mark into ui/");
    }
}
