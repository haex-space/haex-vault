//! Structured agent-task handlers exposed through the extension bridge.

pub(crate) use crate::agent_tasks::bridge::{cancel, start};

pub(crate) const MODULE: &str = module_path!();
