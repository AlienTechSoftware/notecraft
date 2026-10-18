#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]
//! NoteCraft as an MCP server (layer L5).
//!
//! The Model Context Protocol lets an assistant call a program's actions. NoteCraft already
//! has one list of actions, the engine's command registry, so this crate is a thin skin over
//! it: no new behaviour, no second code path, and the same commands the window and the
//! command line run.
//!
//! Two backends:
//!
//! - [`Headless`] opens a notebooks folder itself. Nothing has to be running, which is what a
//!   desktop assistant usually wants.
//! - [`Remote`] talks to a running window over its control channel, so the model sees and
//!   changes what the person is looking at. Tools marked "needs the window" work only here.
//!
//! The transport is JSON-RPC 2.0 in newline-delimited JSON on stdin and stdout, which is what
//! MCP calls stdio. [`serve`] runs the loop; [`handle_line`] answers one line and is what the
//! tests use.

mod backend;
mod server;
mod tools;

pub use backend::{Backend, Headless, Remote, key_dir};
pub use server::{PROTOCOL_VERSION, handle_line, serve};
pub use tools::{TOOLS, Tool};
