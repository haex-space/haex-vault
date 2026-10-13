use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskErrorCode {
    ConsentRequired,
    ConsentDenied,
    UnsupportedCapability,
    ProfileUnavailable,
    InvalidInput,
    InvalidResult,
    Timeout,
    Cancelled,
    InferenceFailed,
    LimitExceeded,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskError {
    pub code: TaskErrorCode,
    pub message: String,
}

impl TaskError {
    pub fn new(code: TaskErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}
