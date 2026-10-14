//! Who asks a service for data (spec 034 research R6, spec 051 research R2). The password manager
//! and the address book check the same callers; what a grant covers stays with each service.

/// Who asks. The caller is the entrance, never an argument of a command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Caller {
    /// The window of the user.
    User,
    /// The model of the holzi chat (spec 032).
    BuiltinAgent,
    /// A haextension (specs 017–019).
    Extension { id: String },
    /// An MCP client (spec 021).
    ExternalAgent { id: String },
    /// A holzi function, such as the own S3 storage (spec 029), with a grant fixed in the code.
    Internal { feature: &'static str },
}
