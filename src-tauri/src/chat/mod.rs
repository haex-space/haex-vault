//! Chat runtime and Tauri commands.
//!
//! Provider-backed chat is available in every build. Local GGUF loading
//! is feature-gated behind `llm-llamacpp`.

pub mod action_commands;
pub mod attachments;
pub mod choices;
pub mod commands;
pub mod compact;
pub mod default_model;
pub mod eval;
pub mod events;
pub mod local_model;
pub mod model_loading;
pub mod peer;
pub mod reply_language;
pub mod search;
pub mod send;
pub mod send_admission;
pub mod session;
pub mod thread_commands;
pub mod tools;
pub mod turn;

#[cfg(test)]
mod session_tests;
