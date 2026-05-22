//! disky core — typed query + render + scan layers shared by the CLI, the TUI
//! and the `disky-mcp` server.
//!
//! Stability: the JSON record shapes exposed by [`query`] are part of the
//! agent-facing contract; bump [`query::SCHEMA_VERSION`] on any breaking
//! change.

pub mod cleanup;
pub mod config;
pub mod db;
pub mod duration;
pub mod exit;
pub mod policy;
pub mod query;
pub mod render;
pub mod scan;
pub mod schema;
pub mod snapshots;
