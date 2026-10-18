//! Model-job handlers exposed through the extension bridge (spec 055).

pub(crate) use crate::model_jobs::bridge::{cancel, list, start};

pub(crate) const MODULE: &str = module_path!();
