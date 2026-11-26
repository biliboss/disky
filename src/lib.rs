//! disky core — typed query + render + scan layers shared by the CLI, the TUI
//! and the `disky-mcp` server.
//!
//! Stability: the JSON record shapes exposed by [`query`] are part of the
//! agent-facing contract; bump [`query::SCHEMA_VERSION`] on any breaking
//! change.

pub mod agents;
pub mod assistant;
pub mod cleanup;
pub mod config;
pub mod contract;
pub mod db;
pub mod disk;
pub mod dupes;
pub mod duration;
pub mod envelope;
pub mod exit;
pub mod filter;
pub mod history;
pub mod ignore;
pub mod large;
pub mod map;
pub mod pattern;
pub mod platform;
pub mod policy;
pub mod predict;
pub mod prefs;
pub mod query;
pub mod render;
pub mod scan;
pub mod schema;
pub mod snapshots;
pub mod trash;
