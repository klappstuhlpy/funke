//! Small utility providers. None of them index anything, so they live together in one
//! crate; the only state anywhere is the web provider's cached default-browser icon.

mod calc;
mod system;
mod web;

pub use calc::CalcProvider;
pub use system::SystemProvider;
pub use web::{WebSearchProvider, ENGINES};
