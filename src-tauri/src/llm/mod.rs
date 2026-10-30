//! Local and provider-backed LLM inference.
//!
//! The local submodule is gated behind `llm-cpu` so a
//! `--no-default-features` build can still use provider-backed chat.

#[cfg(feature = "llm-cpu")]
pub(crate) mod holdback;
#[cfg(feature = "llm-cpu")]
pub mod local;

#[cfg(feature = "llm-llamacpp")]
pub mod llamacpp;
