use crate::extension::permissions::checker::matches_target;

#[test]
fn test_matches_target_exact() {
    assert!(matches_target("exact_table", "exact_table"));
    assert!(!matches_target("exact_table", "different_table"));
    assert!(!matches_target("exact_table", "exact_table_extended"));
}

#[test]
fn test_matches_target_prefix_wildcard() {
    assert!(matches_target("prefix__*", "prefix__table1"));
    assert!(matches_target("prefix__*", "prefix__table2"));
    assert!(matches_target("prefix__*", "prefix__deeply__nested__table"));
    assert!(!matches_target("prefix__*", "other__table"));
    assert!(!matches_target("prefix__*", "prefixNOSEPARATOR__table"));
}

#[test]
fn test_matches_target_full_wildcard() {
    // Full wildcard should match non-system tables
    assert!(matches_target("*", "any_table"));
    assert!(matches_target("*", "custom_user_data"));

    // But NOT system tables (checked separately)
    assert!(!matches_target("*", "haex_extensions"));
    assert!(!matches_target("*", "sqlite_master"));
}

#[test]
fn test_matches_target_does_not_match_system_tables() {
    // Verify that wildcard patterns don't match system tables
    let patterns = ["*", "haex_*", "sqlite_*", "h*", "s*"];
    let system_tables = ["haex_extensions", "haex_vault_settings", "sqlite_master"];

    for pattern in patterns {
        for table in system_tables {
            assert!(
                !matches_target(pattern, table),
                "Pattern '{}' should NOT match system table '{}'",
                pattern,
                table
            );
        }
    }
}
