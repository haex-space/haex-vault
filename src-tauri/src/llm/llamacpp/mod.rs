//! The application's local GGUF engine: llama.cpp, behind the feature `llm-llamacpp`.
//!
//! It loads GGUF models and implements the app's `ProviderAdapter` for local chat.
//!
//! - `model`: the backend of the process, the load and the memory plan (`memory`).
//! - `cpu`: which processors run this build's ggml, and the thread count.
//! - `generate`: the token loop, with cancelling; `speculative`: the same with an MTP drafter (spec 079).
//! - `streaming`: generated text into reasoning, content and tool calls (`formats`).
//! - `adapter`: `LlamaCppAdapter` over all of it.

pub mod adapter;
pub mod choice;
mod context_params;
pub mod cpu;
pub mod formats;
pub mod generate;
pub mod memory;
pub mod model;
pub mod report;
// The binding's speculative decoding needs libcommon, which only desktops build (spec 079).
#[cfg(not(any(target_os = "android", target_os = "ios")))]
mod speculative;
pub mod streaming;
pub mod template;

pub use adapter::LlamaCppAdapter;
pub use model::{LoadError, LoadOptions};

#[cfg(test)]
#[path = "links_tests.rs"]
mod links_tests;
#[cfg(test)]
#[path = "roundtrip_tests.rs"]
mod roundtrip_tests;
