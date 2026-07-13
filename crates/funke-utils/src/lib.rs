//! Small utility providers. None of them index anything, so they live together in one
//! crate; the only state anywhere is the default browser's icon, which web search and
//! quicklinks share ([`browser`]) because both of them end in the browser.

mod browser;
mod calc;
mod links;
mod system;
mod units;
mod web;

pub use calc::CalcProvider;
pub use links::QuicklinksProvider;
pub use system::SystemProvider;
pub use web::{WebSearchProvider, ENGINES};
