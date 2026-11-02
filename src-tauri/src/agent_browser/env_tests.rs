use super::validate_builtin_password_rights;
use crate::mcp_server::grants::{PasswordLevel, PasswordRights};

#[test]
fn built_in_password_rights_are_read_only() {
    assert!(validate_builtin_password_rights(None).is_ok());
    assert!(validate_builtin_password_rights(Some(&PasswordRights {
        level: PasswordLevel::Read,
        tags: None,
    }))
    .is_ok());
    assert!(validate_builtin_password_rights(Some(&PasswordRights {
        level: PasswordLevel::ReadWrite,
        tags: None,
    }))
    .is_err());
}
