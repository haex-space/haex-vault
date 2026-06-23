use super::helpers::create_extension;
use crate::extension::permissions::checker::PermissionChecker;
use crate::extension::permissions::types::DbAction;

#[test]
fn test_extension_isolation_by_prefix() {
    // Create two extensions
    let ext_a = create_extension("pubkey_a", "ext_a");
    let ext_b = create_extension("pubkey_b", "ext_b");

    let checker_a = PermissionChecker::new(ext_a, vec![]);
    let checker_b = PermissionChecker::new(ext_b, vec![]);

    // Extension A can only access its own tables
    assert!(checker_a.can_access_table("pubkey_a__ext_a__users", DbAction::ReadWrite));
    assert!(!checker_a.can_access_table("pubkey_b__ext_b__users", DbAction::Read));

    // Extension B can only access its own tables
    assert!(checker_b.can_access_table("pubkey_b__ext_b__users", DbAction::ReadWrite));
    assert!(!checker_b.can_access_table("pubkey_a__ext_a__users", DbAction::Read));
}

#[test]
fn test_extension_cannot_impersonate_prefix() {
    // Extension with similar but different prefix
    let extension = create_extension("pubkey", "myext");
    let checker = PermissionChecker::new(extension, vec![]);

    // Cannot access tables with slightly different prefix
    assert!(!checker.can_access_table("pubkey__myext_extra__users", DbAction::Read)); // extra underscore
    assert!(!checker.can_access_table("pubkey_myext__users", DbAction::Read)); // missing double underscore
    assert!(!checker.can_access_table("pubkey2__myext__users", DbAction::Read));
    // different pubkey
}
