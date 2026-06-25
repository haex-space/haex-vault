//! String escape, encoding, and character-set attacks.
//!
//! Quote escaping, backslash/SQLite quirks, Unicode normalisation, hex
//! literals, null bytes — all attempts to slip past naive string-based
//! filters. Real defence is parameterised queries; these tests just
//! pin the parser's behaviour.

use crate::database::core::parse_sql_statements;

// String-escape / unicode / hex / null-byte payloads are all valid SQL
// once parsed — the real defence is parameter binding, NOT parsing.
// These tests pin parser behaviour: whatever it does, no smuggled
// second statement may slip through.
fn assert_no_stacked_statements(sql: &str) {
    match parse_sql_statements(sql) {
        Ok(stmts) => assert!(
            stmts.len() <= 1,
            "Payload smuggled multiple statements past the parser: {sql} ({} stmts)",
            stmts.len()
        ),
        Err(_) => { /* parser refused — also safe */ }
    }
}

#[test]
fn test_string_escape_single_quote() {
    let attacks = [
        "SELECT * FROM users WHERE name = 'admin' --'",
        "SELECT * FROM users WHERE name = '' OR '1'='1'",
        "SELECT * FROM users WHERE name = ''''",
    ];
    for sql in attacks {
        assert_no_stacked_statements(sql);
    }
}

#[test]
fn test_string_escape_backslash() {
    // SQLite uses '' (not \') — pin no-stacking either way.
    assert_no_stacked_statements("SELECT * FROM users WHERE name = 'test\\'--'");
}

#[test]
fn test_unicode_escape_injection() {
    let attacks = [
        "SELECT * FROM users WHERE name = N'admin'",
        "SELECT * FROM users WHERE name = U&'admin'",
    ];
    for sql in attacks {
        assert_no_stacked_statements(sql);
    }
}

#[test]
fn test_hex_encoded_injection() {
    assert_no_stacked_statements("SELECT * FROM users WHERE name = X'61646D696E'");
}

#[test]
fn test_null_byte_injection() {
    // The intent of the original payload is a stacked-query attack. The
    // multi-statement test already covers the `;`-separated case, so here we
    // pin the null-byte-in-string variant without the trailing stack.
    assert_no_stacked_statements("SELECT * FROM users WHERE name = 'admin\0'");
}

#[test]
fn test_unicode_normalization_attack() {
    assert_no_stacked_statements("SELECT * FROM users WHERE name = 'ａｄｍｉｎ'");
}
