/// A permission principal — the actor a permission check is performed against.
///
/// Today every principal is an extension (`principal_id == extension_id`), so
/// the permission layer behaves exactly as before. `ExternalClient` is wired in
/// ahead of the external-bridge work where clients become first-class
/// principals sharing the same `haex_principal_permissions` machinery.
//
// `ExternalClient` + `kind_str` are not constructed/called in production yet
// (only in unit tests) — they exist for the upcoming external-client phases.
#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq)]
pub enum Principal {
    Extension(String),
    ExternalClient(String),
}

impl Principal {
    /// The principal's id — the value stored in `haex_principal_permissions.principal_id`.
    pub fn id(&self) -> &str {
        match self {
            Self::Extension(i) | Self::ExternalClient(i) => i,
        }
    }

    /// The principal kind as it is persisted in `haex_principals.kind`.
    #[allow(dead_code)]
    pub fn kind_str(&self) -> &'static str {
        match self {
            Self::Extension(_) => "extension",
            Self::ExternalClient(_) => "external_client",
        }
    }

    /// Whether this principal is an extension. Used to gate extension-only
    /// behaviour (e.g. auto-allowed own tables) that external clients lack.
    pub fn is_extension(&self) -> bool {
        matches!(self, Self::Extension(_))
    }
}
