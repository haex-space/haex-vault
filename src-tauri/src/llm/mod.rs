//! Local and provider-backed LLM inference.

#[cfg(feature = "llm-llamacpp")]
pub mod engine;
#[cfg(feature = "llm-llamacpp")]
pub(crate) mod gguf_header;
#[cfg(all(test, feature = "llm-llamacpp"))]
mod gguf_header_tests;
#[cfg(feature = "llm-llamacpp")]
pub(crate) mod holdback;
#[cfg(feature = "llm-llamacpp")]
pub mod llamacpp;
#[cfg(feature = "llm-llamacpp")]
pub(crate) mod probe;
#[cfg(all(test, feature = "llm-llamacpp"))]
mod probe_tests;
