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
    println!("cargo:rerun-if-changed=icons/icon.png");
    let bytes = std::fs::read(Path::new("icons/icon.png")).expect("icons/icon.png is the app mark");
    let staged = Path::new("ui/icon.png");
    if std::fs::read(staged).ok().as_deref() != Some(&bytes) {
        std::fs::write(staged, &bytes).expect("failed to stage the app mark into ui/");
    }
}
