//! Windows shell helpers shared by provider crates. Anything COM-flavored lives here so
//! `funke-core` stays platform-pure and providers don't reimplement icon plumbing.

mod browser;
mod icon;
mod uia;

pub use browser::{default_browser_exe, default_browser_icon};
pub use icon::icon_data_url;
pub use uia::{browser_url, is_browser_process};
