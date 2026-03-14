//! Error types for device identity management

#[derive(Debug, thiserror::Error)]
pub enum DeviceError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Encryption error: {reason}")]
    Encryption { reason: String },

    #[error("Device key error: {reason}")]
    KeyError { reason: String },

    #[error("Database error: {reason}")]
    Database { reason: String },
}

impl serde::Serialize for DeviceError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}
