//! llama.cpp as a second local engine (spec 070, ADR 0023), behind the feature `llm-llamacpp`.
//!
//! It loads the GGUF architectures mistralrs cannot (Qwen3.5, Gemma 4) and sits behind the same
//! `ProviderAdapter` as the mistralrs path. The pieces arrive stage by stage (`specs/070-llama-cpp-engine/
//! tasks.md`): the template renderer and the call formats (PR B), the adapter (PR C), the choice of engine
//! (PR D). Until then the prototype in `llm/local/llamacpp_spike_adapter.rs` serves the evaluation.

#[cfg(test)]
#[path = "links_tests.rs"]
mod links_tests;
