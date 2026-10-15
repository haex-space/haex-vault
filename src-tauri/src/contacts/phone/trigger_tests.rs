use super::*;

#[test]
fn changes_of_contacts_start_a_run_but_the_runs_own_rows_do_not() {
    for table in [
        "haex_contacts",
        "haex_contacts_values",
        "haex_contacts_group_members",
        "haex_contacts_groups",
        "haex_contacts_targets",
        "haex_contacts_binaries",
    ] {
        assert!(concerns_phone(table), "{table}");
    }
    for table in [
        "haex_contacts_phone_links",
        "haex_contacts_phone_issues",
        "haex_contacts_phone_runs",
        "haex_contacts_phone_known_no_sync",
        "preferences",
        "haex_passwords_items",
    ] {
        assert!(!concerns_phone(table), "{table}");
    }
}
