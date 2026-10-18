#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]
//! NoteCraft engine (layer L4): the session and the command registry.
//!
//! **Everything is a command.** Every user-visible action is a `CommandSpec` with an
//! id, a label, a documented parameter list and a `run` function. The window, the
//! command line and (later) a control channel or MCP server all dispatch by id, so
//! one test covers all of them. Programmatic calls never open dialogs.
//!
//! `Session::run` also catches a panic that escapes a command and turns it into an
//! error, so a bug cannot take the app down. That is a safety net, not a licence:
//! the command code itself avoids `unwrap`, `expect` and `panic`.

mod commands;
mod error;
mod json;
pub mod sample;
mod session;
pub mod welcome;

pub use commands::{CommandSpec, Output, Params, registry};
pub use error::EngineError;
pub use json::{commands_json, output_json, params_from_json};
pub use session::Session;
