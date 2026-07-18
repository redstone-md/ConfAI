//! ConfAI — one editor for every AI coding agent's config.
//!
//! The layers, outermost first:
//!
//! - [`cli`] parses arguments and [`commands`] carries them out.
//! - [`tui`] is the same operations behind an interactive list.
//! - [`agent`] holds one backend per CLI, each mapping its own file format onto
//!   the agent-neutral types in [`domain`].
//! - [`net`] asks providers what they serve and models.dev how big it is.
//! - [`store`] is the only place that writes to disk.

pub mod agent;
pub mod brand;
pub mod cli;
pub mod commands;
pub mod domain;
pub mod mcp;
pub mod net;
pub mod preset;
pub mod store;
pub mod tui;
pub mod ui;
pub mod update;
