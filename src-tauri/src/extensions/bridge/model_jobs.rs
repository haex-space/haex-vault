//! Structured agent-task handlers exposed through the extension bridge.

pub(crate) use crate::model_jobs::bridge::{cancel, start};

pub(crate) const MODULE: &str = module_path!();
