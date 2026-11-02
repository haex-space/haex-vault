use super::validate_builtin_password_rights;
use crate::mcp_server::grants::{PasswordLevel, PasswordRights};

fn rights(
    level: PasswordLevel,
    tags: Option<&[&str]>,
    default_tag: Option<&str>,
) -> PasswordRights {
    PasswordRights {
        level,
        tags: tags.map(|tags| tags.iter().map(|tag| (*tag).to_owned()).collect()),
        default_tag: default_tag.map(str::to_owned),
    }
}

/// Spec 065 FR-036 (operator 2026-11-02): the built-in agent may read or also save new logins,
/// with the same rules as an external agent.
#[test]
fn built_in_password_rights_follow_the_rules_of_agents() {
    assert!(validate_builtin_password_rights(None).is_ok());
    assert!(
        validate_builtin_password_rights(Some(&rights(PasswordLevel::Read, None, None))).is_ok()
    );
    assert!(
        validate_builtin_password_rights(Some(&rights(PasswordLevel::ReadWrite, None, None)))
            .is_ok()
    );
    assert!(validate_builtin_password_rights(Some(&rights(
        PasswordLevel::ReadWrite,
        Some(&["shop"]),
        Some("shop")
    )))
    .is_ok());
    assert!(validate_builtin_password_rights(Some(&rights(
        PasswordLevel::ReadWrite,
        Some(&["shop"]),
        None
    )))
    .is_err());
}
