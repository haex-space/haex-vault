//! llama.cpp as a second local engine (spec 070, ADR 0023), behind the feature `llm-llamacpp`.
//!
//! It loads the GGUF architectures mistralrs cannot (Qwen3.5, Gemma 4) and sits behind the same
//! `ProviderAdapter` as the mistralrs path. The pieces arrive stage by stage (`specs/070-llama-cpp-engine/
//! tasks.md`): the template renderer and the call formats (PR B, here), the adapter (PR C), the choice of
//! engine (PR D). Until the adapter lands, the prototype in `llm/local/llamacpp_spike_adapter.rs` serves the
//! evaluation and uses these modules.

pub mod formats;
pub mod template;

#[cfg(test)]
#[path = "links_tests.rs"]
mod links_tests;
#[cfg(test)]
#[path = "roundtrip_tests.rs"]
mod roundtrip_tests;
