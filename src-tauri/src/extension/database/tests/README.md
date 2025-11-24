# Extension Database Security Tests

This directory contains security tests for the Extension Database Migration system.

## Test File Structure

```
database/tests/
├── mod.rs                # Test module declarations
├── sql_parsing_tests.rs  # SQL parsing security tests (15 tests)
├── executor_tests.rs     # SqlExecutor tests (placeholder with #[ignore])
└── README.md            # This file
```

## Test Coverage

### 1. SQL Parsing Security Tests (`sql_parsing_tests.rs`)
**15 Automated Tests** - Ensures malicious SQL patterns are detected at parsing level

- ✅ `test_parse_single_valid_statement` - Valid single statements parse
- ✅ `test_reject_multiple_statements` - Multiple statements detected/rejected
- ✅ `test_reject_union_injection` - UNION injection attempts detected
- ✅ `test_parse_create_with_foreign_key` - Foreign keys are supported
- ✅ `test_parse_create_index` - CREATE INDEX statements work
- ✅ `test_reject_completely_invalid_sql` - Garbage input is rejected
- ✅ `test_parse_comment_injection` - SQL comments are handled safely
- ✅ `test_parse_nested_select` - Nested SELECT queries work
- ✅ `test_parse_with_cte` - Common Table Expressions (CTE) handling
- ✅ `test_parse_trigger_statement` - TRIGGER statements detected (rejected by validator)
- ✅ `test_parse_with_unicode_characters` - Unicode in identifiers handled
- ✅ `test_parse_with_quoted_identifiers` - Quoted identifiers work
- ✅ `test_parse_with_backtick_identifiers` - Backtick identifiers work
- ✅ `test_detect_attach_database_attempt` - ATTACH DATABASE detected
- ✅ `test_detect_pragma_attempt` - PRAGMA statements detected

##  Security Architecture

The Extension Database security system has multiple layers:

### Layer 1: SQL Parsing (✅ Automated Tests)
- Validates SQL syntax
- Detects multiple statement injection attempts
- Handles edge cases (comments, CTEs, unicode, quoted identifiers)
- Tests: `sql_parsing_tests.rs` (15 tests)

### Layer 2: Permission Validation (⚠️ Manual Testing Required)
The following security guarantees are implemented but require manual/integration testing due to Tauri State dependencies:

1. **Table Isolation**: Extensions can ONLY access tables with their own prefix `{public_key}__{extension_name}__*`
   - Implemented in: [`SqlPermissionValidator::validate_sql()`](../mod.rs)
   - Check: [`PermissionManager::check_database_permission()`](../../permissions/manager.rs#L194-L248)

2. **System Protection**: System tables (`haex_*`) and `sqlite_master` cannot be accessed
   - Auto-deny logic: Lines 217-222 in `permissions/manager.rs`
   - Extensions without explicit permission to system tables are blocked

3. **No Cross-Extension Access**: Extensions cannot read/write other extensions' data
   - Table prefix validation ensures isolation
   - Each extension's `expected_prefix` is `{public_key}__{name}__`

4. **Migration Security**: Only extensions with CREATE permission can register migrations
   - Validated in: [`register_extension_migrations()`](../mod.rs#L342-L405)
   - Calls `SqlPermissionValidator::validate_sql()` before storing

5. **Malicious Migration Detection**: Migrations targeting system tables are rejected
   - Same validation as regular SQL statements
   - CREATE permission doesn't grant access to system tables

## Running Automated Tests

```bash
# Run all extension database tests (15 tests)
cargo test extension::database::tests --lib

# Run SQL parsing tests specifically
cargo test extension::database::tests::sql_parsing_tests --lib

# Run with output
cargo test extension::database::tests --lib -- --nocapture
```

## Manual Testing Guide

Due to Tauri State dependencies, permission validation logic requires manual testing. Here's how to verify each security guarantee:

### Test 1: Extension Can Create Own Table ✓
1. Install a test extension with `database.create` permission
2. Use extension SDK to execute: `CREATE TABLE {public_key}__{name}__test_table (id TEXT)`
3. **Expected**: SUCCESS - table created

### Test 2: Extension Cannot Create Wrong Prefix Table ✗
1. Same extension from Test 1
2. Try to execute: `CREATE TABLE wrong_prefix__table (id TEXT)`
3. **Expected**: PermissionDenied error

### Test 3: Extension Cannot Access System Tables ✗
1. Extension with `database.read` permission on `*`
2. Try to execute: `SELECT * FROM haex_extensions`
3. **Expected**: PermissionDenied error
4. Repeat for other system tables: `haex_extension_permissions`, `haex_vault_settings`, `haex_passwords`

### Test 4: Extension Cannot Access sqlite_master ✗
1. Extension with `database.read` permission on `*`
2. Try to execute: `SELECT * FROM sqlite_master`
3. **Expected**: PermissionDenied error

### Test 5: Extension Cannot Access Other Extension Tables ✗
1. Create Extension A and Extension B (different public_keys/names)
2. Extension A creates table: `{pubkey_a}__{name_a}__private`
3. Extension B tries to SELECT from that table
4. **Expected**: PermissionDenied error

### Test 6: Valid Migrations Succeed ✓
1. Extension with `database.create` permission
2. Register migration via SDK:
   ```typescript
   await database.registerMigrations([{
     name: "0000_initial",
     sql: "CREATE TABLE {prefix}__users (id TEXT PRIMARY KEY)"
   }])
   ```
3. **Expected**: SUCCESS - migration stored in `haex_extension_migrations`

### Test 7: Malicious Migrations Rejected ✗
1. Extension with `database.create` permission
2. Try to register malicious migration:
   ```typescript
   await database.registerMigrations([{
     name: "0000_malicious",
     sql: "CREATE TABLE haex_stolen_data (password TEXT)"
   }])
   ```
3. **Expected**: PermissionDenied error - migration NOT stored

### Test 8: Extensions Without CREATE Permission Cannot Register Migrations ✗
1. Extension with ONLY `database.read` permission (no create)
2. Try to register ANY migration
3. **Expected**: PermissionDenied error

## Verifying Test Results

After manual testing, verify results by checking:

```sql
-- Check extension was registered
SELECT id, name, public_key FROM haex_extensions;

-- Check permissions were granted correctly
SELECT extension_id, resource_type, action, target, status
FROM haex_extension_permissions
WHERE extension_id = '{your_extension_id}';

-- Check which migrations were stored (should only be valid ones)
SELECT extension_id, migration_name, sql_statement
FROM haex_extension_migrations;

-- Check no malicious tables were created
SELECT name FROM sqlite_master
WHERE type = 'table' AND name LIKE 'haex_%';
```

## Adding New Tests

When adding new tests:

1. Create a new test file in `src/extension/database/tests/`
2. Register it in `mod.rs`: `mod your_test_file;`
3. Follow the naming convention: `test_descriptive_name`
4. Add documentation explaining what security aspect is tested

## Test Architecture

### Automated Tests (`sql_parsing_tests.rs`)
- **Layer**: SQL Parsing
- **Coverage**: Syntax validation, injection detection, edge cases
- **Execution**: Fast, runs in CI/CD
- **Purpose**: Ensures malicious SQL patterns are detected before reaching permission layer

### Manual Integration Testing
- **Layer**: Permission Validation + Database Operations
- **Coverage**: Table isolation, system protection, cross-extension access
- **Execution**: Requires running application with test extensions
- **Purpose**: Verifies permission logic works correctly in production environment

**Why Manual Testing?**
- Tauri State management makes unit testing complex
- Permission checks depend on database state and ExtensionManager
- Real-world testing provides better confidence than mocked integration tests
- Manual testing guide ensures consistent validation process
