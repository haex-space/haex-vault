//! In-process GGUF inference via mistralrs 0.8.x.
//!
//! Wraps [`mistralrs::Model`] for holzi's use case: single-model load,
//! token-by-token streaming into an [`crate::adapters::types::AdapterStream`],
//! cancellable by dropping the stream. Backend selection (CPU/CUDA/Metal)
//! happens at compile time via feature flags — the runtime cannot
//! upgrade CPU to CUDA without a rebuild (Etappe 0 finding #3).

pub(crate) mod gguf_header;
#[cfg(test)]
mod gguf_header_tests;
mod loader;
#[cfg(test)]
mod loader_tests;
#[cfg(test)]
mod multimodal_spike_tests;
pub(crate) mod probe;
#[cfg(test)]
mod probe_tests;
mod stream;

pub use loader::{LocalModel, LocalModelError};
