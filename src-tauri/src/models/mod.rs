//! Model file management: download from HuggingFace, import from disk,
//! path resolution under `<AppLocalData>/models/<slug>/`.
//!
//! Registry rows in `device_downloaded_models_no_sync` reference files
//! by relative path — this module owns the absolute-path resolution so
//! callers do not embed the sandboxing rules of the host OS.

pub mod cancel;
pub mod commands;
#[cfg(feature = "llm-llamacpp")]
pub mod context_commands;
#[cfg(feature = "llm-llamacpp")]
pub mod context_setting;
pub mod download;
pub mod download_check;
pub mod drafter;
pub mod hash;
pub mod huggingface;
#[cfg(test)]
mod huggingface_install_tests;
#[cfg(test)]
mod huggingface_search_tests;
pub mod import;
pub mod paths;
pub mod stand_in;
