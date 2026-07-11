//! The Funke plugin system: out-of-process providers speaking JSON-RPC 2.0 over stdio
//! (docs/PLUGINS.md is the authoring guide, docs/DESIGN.md §3 the rationale — plugins are
//! separate executables in any language; no dylibs, no stable-ABI problems, crash
//! isolation for free).
//!
//! Four modules:
//! - [`proto`] — the wire types both sides share.
//! - [`sdk`] — what a Rust plugin author uses: implement [`sdk::Plugin`], call
//!   [`sdk::serve`] in `main`, done.
//! - [`host`] — what the launcher uses: discover manifests, spawn plugin processes
//!   lazily, adapt each one into a `funke_core::SearchProvider`.
//! - [`catalog`] — the curated index of installable plugins: fetch, hash-verify, unpack.

pub mod catalog;
pub mod host;
pub mod proto;
pub mod sdk;
