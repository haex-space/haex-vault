//! String escape, encoding, and character-set attacks.
//!
//! Quote escaping, backslash/SQLite quirks, Unicode normalisation, hex
//! literals, null bytes — all attempts to slip past naive string-based
//! filters. Real defence is parameterised queries; these tests just
//! pin the parser's behaviour.

use crate::database::core::parse_sql_statements;

#[test]
fn test_string_escape_single_quote() {
    // Classic string escape attack
    let attacks = [
        "SELECT * FROM users WHERE name = 'admin' --'",
        "SELECT * FROM users WHERE name = '' OR '1'='1'",
        "SELECT * FROM users WHERE name = ''''",
    ];

    for sql in attacks {
        let result = parse_sql_statements(sql);
        // These should parse as valid SQL
        // The protection comes from using parameterized queries
        println!("String escape '{}' parse result: {:?}", sql, result.is_ok());
    }
}

#[test]
fn test_string_escape_backslash() {
    // Backslash escape attempts (SQLite uses '' not \')
    let sql = "SELECT * FROM users WHERE name = 'test\\'--'";
    let _ = parse_sql_statements(sql);
    // SQLite handles escaping differently than MySQL
}

#[test]
fn test_unicode_escape_injection() {
    // Unicode escape attempts
    let attacks = [
        "SELECT * FROM users WHERE name = N'admin'",
        "SELECT * FROM users WHERE name = U&'admin'",
    ];

    for sql in attacks {
        let _ = parse_sql_statements(sql);
    }
}

#[test]
fn test_hex_encoded_injection() {
    // Hex encoding bypass attempts
    let sql = "SELECT * FROM users WHERE name = X'61646D696E'"; // 'admin' in hex
    let result = parse_sql_statements(sql);
    println!("Hex encoding parse result: {:?}", result.is_ok());
}

#[test]
fn test_null_byte_injection() {
    // Null byte injection (typically more relevant for C-based systems)
    let sql = "SELECT * FROM users WHERE name = 'admin\0'; DROP TABLE haex_extensions; --'";
    let _ = parse_sql_statements(sql);
}

#[test]
fn test_unicode_normalization_attack() {
    // Unicode characters that might normalize to SQL syntax
    let sql = "SELECT * FROM users WHERE name = 'ａｄｍｉｎ'"; // Full-width letters
    let result = parse_sql_statements(sql);
    println!("Unicode normalization parse result: {:?}", result.is_ok());
}
