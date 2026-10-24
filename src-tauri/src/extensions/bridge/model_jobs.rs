//! Model-job handlers exposed through the extension bridge (spec 055), with the way to the grant
//! (US9).

pub(crate) use crate::model_jobs::access::{open_settings, request as access_request};
pub(crate) use crate::model_jobs::bridge::{cancel, list, start};

pub(crate) const MODULE: &str = module_path!();
