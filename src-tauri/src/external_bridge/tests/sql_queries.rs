//! SQL query constant tests — placeholders, table references, and
//! the extension-targeted authorization / lookup queries.

use super::super::authorization::*;

#[test]
fn test_sql_queries_are_valid() {
    // These tests verify that the SQL queries have correct placeholders
    assert!(SQL_IS_AUTHORIZED.contains("?1"));
    assert!(SQL_IS_AUTHORIZED.contains("?2"));
    assert!(SQL_IS_CLIENT_KNOWN.contains("?1"));
    assert!(SQL_GET_CLIENT_EXTENSION.contains("?1"));
    assert!(SQL_GET_CLIENT.contains("?1"));
    assert!(SQL_INSERT_CLIENT.contains("?1"));
    assert!(SQL_INSERT_CLIENT.contains("?2"));
    assert!(SQL_INSERT_CLIENT.contains("?3"));
    assert!(SQL_INSERT_CLIENT.contains("?4"));
    assert!(SQL_INSERT_CLIENT.contains("?5"));
    assert!(SQL_UPDATE_LAST_SEEN.contains("?1"));
    assert!(SQL_DELETE_CLIENT.contains("?1"));
}

#[test]
fn test_sql_queries_reference_correct_table() {
    let table_name = crate::table_names::TABLE_EXTERNAL_AUTHORIZED_CLIENTS;
    assert!(SQL_IS_AUTHORIZED.contains(table_name));
    assert!(SQL_IS_CLIENT_KNOWN.contains(table_name));
    assert!(SQL_GET_CLIENT_EXTENSION.contains(table_name));
    assert!(SQL_GET_CLIENT.contains(table_name));
    assert!(SQL_GET_ALL_CLIENTS.contains(table_name));
    assert!(SQL_INSERT_CLIENT.contains(table_name));
    assert!(SQL_UPDATE_LAST_SEEN.contains(table_name));
    assert!(SQL_DELETE_CLIENT.contains(table_name));
}

#[test]
fn test_sql_is_client_authorized_for_extension_query_format() {
    // Verify the new query for checking client authorization for specific extension
    let query = &*SQL_IS_CLIENT_AUTHORIZED_FOR_EXTENSION;

    // Should have all three placeholders
    assert!(
        query.contains("?1"),
        "Query should have ?1 placeholder for client_id"
    );
    assert!(
        query.contains("?2"),
        "Query should have ?2 placeholder for extension public_key"
    );
    assert!(
        query.contains("?3"),
        "Query should have ?3 placeholder for extension name"
    );

    // Should reference both tables
    assert!(
        query.contains(crate::table_names::TABLE_EXTERNAL_AUTHORIZED_CLIENTS),
        "Query should reference authorized clients table"
    );
    assert!(
        query.contains("haex_extensions"),
        "Query should reference extensions table"
    );

    // Should use JOIN
    assert!(
        query.to_lowercase().contains("join"),
        "Query should use JOIN"
    );

    // Should check public_key and name columns of extensions table
    assert!(
        query.contains("public_key"),
        "Query should check public_key"
    );
    assert!(query.contains("name"), "Query should check name");
}

#[test]
fn test_sql_get_extension_id_by_public_key_and_name_query_format() {
    // Verify the query for looking up extension ID by public_key and name
    let query = &*SQL_GET_EXTENSION_ID_BY_PUBLIC_KEY_AND_NAME;

    // Should have both placeholders
    assert!(
        query.contains("?1"),
        "Query should have ?1 placeholder for public_key"
    );
    assert!(
        query.contains("?2"),
        "Query should have ?2 placeholder for name"
    );

    // Should reference extensions table
    assert!(
        query.contains("haex_extensions"),
        "Query should reference extensions table"
    );

    // Should select id
    assert!(
        query.to_lowercase().contains("select id"),
        "Query should select id"
    );

    // Should filter by public_key and name
    assert!(
        query.contains("public_key"),
        "Query should filter by public_key"
    );
    assert!(query.contains("name"), "Query should filter by name");
}

#[test]
fn test_sql_blocked_clients_queries_reference_correct_table() {
    let table_name = crate::table_names::TABLE_EXTERNAL_BLOCKED_CLIENTS;
    assert!(SQL_IS_BLOCKED.contains(table_name));
    assert!(SQL_GET_BLOCKED_CLIENT.contains(table_name));
    assert!(SQL_GET_ALL_BLOCKED_CLIENTS.contains(table_name));
    assert!(SQL_INSERT_BLOCKED_CLIENT.contains(table_name));
    assert!(SQL_DELETE_BLOCKED_CLIENT.contains(table_name));
}
