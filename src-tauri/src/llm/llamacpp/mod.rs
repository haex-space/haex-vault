//! llama.cpp as a second local engine (spec 070, ADR 0023), behind the feature `llm-llamacpp`.
//!
//! It loads the GGUF architectures mistralrs cannot (Qwen3.5, Gemma 4) and sits behind the same
//! `ProviderAdapter` as the mistralrs path. The pieces arrive stage by stage (`specs/070-llama-cpp-engine/
//! tasks.md`): the template renderer and the call formats (PR B), the adapter (PR C, here), the choice of
//! engine (PR D).
//!
//! - `model`: the backend of the process, the load and the memory plan (`memory`).
//! - `cpu`: which processors run this build's ggml, and the thread count.
//! - `generate`: the token loop, with cancelling.
//! - `streaming`: generated text into reasoning, content and tool calls (`formats`).
//! - `adapter`: `LlamaCppAdapter` over all of it.

pub mod adapter;
pub mod cpu;
pub mod formats;
pub mod generate;
pub mod memory;
pub mod model;
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
