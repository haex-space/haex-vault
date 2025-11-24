# Extension Permission Testing Architecture

## Overview

This document describes the comprehensive testing architecture for the Extension Permission System, which ensures that extensions can only access their own tables and that cross-extension access is properly controlled.

## Module Structure

```
permissions/
├── commands.rs          # Tauri Commands (Frontend-Interface)
├── checker.rs           # Testable business logic
├── manager.rs           # Permission CRUD operations
├── validator.rs         # SQL validation
├── types.rs             # Type definitions
├── tests/               # Unit tests
│   ├── mod.rs
│   └── checker_tests.rs # Tests for PermissionChecker
└── TESTING.md           # This file
```

## Architecture

### 1. Testable Core Logic (`checker.rs`)

The `PermissionChecker` struct provides testable permission checking logic without any Tauri State dependencies:

```rust
pub struct PermissionChecker {
    pub extension: Extension,
    pub permissions: Vec<ExtensionPermission>,
}
```

#### Key Methods:
- `can_access_table(table_name, action)` - Validates table access permissions
- `can_create_tables()` - Checks if extension has CREATE permission
- `validate_table_name(table_name)` - Validates if a table name is allowed

#### Permission Rules:
1. **Own Tables**: Extensions have automatic access to tables with their prefix `{public_key}__{extension_name}__`
2. **System Tables**: `haex_*` tables are always blocked, even with wildcards
3. **Explicit Permissions**: Support for wildcards:
   - `*` - Grants access to all non-system tables
   - `prefix__*` - Grants access to all tables starting with prefix
   - `exact_table` - Grants access to specific table

### 2. Centralized Utilities (`utils.rs`)

Common table prefix operations are centralized to avoid code duplication:

```rust
/// Generates the table prefix for an extension
pub fn get_extension_table_prefix(public_key: &str, extension_name: &str) -> String

/// Checks if a table name belongs to a specific extension
pub fn is_extension_table(table_name: &str, public_key: &str, extension_name: &str) -> bool
```

### 3. Integration with Production Code

The `PermissionManager::check_database_permission()` uses the testable checker:

```rust
pub async fn check_database_permission(
    app_state: &State<'_, AppState>,
    extension_id: &str,
    action: Action,
    table_name: &str,
) -> Result<(), ExtensionError> {
    let extension = app_state.extension_manager.get_extension(extension_id)?.clone();
    let permissions = Self::get_permissions(app_state, extension_id).await?;

    let checker = PermissionChecker::new(extension, permissions);
    checker.can_access_table(table_name, db_action)
        .then_some(())
        .ok_or_else(|| ExtensionError::permission_denied(...))
}
```

## Test Coverage

### Layer 1: SQL Parsing Tests (`sql_parsing_tests.rs`)
15 tests validating SQL parsing security:
- Multiple statement detection
- SQL injection patterns (UNION, ATTACH DATABASE)
- Comment injection
- PRAGMA commands
- Trigger statements

### Layer 2: Permission Logic Tests (`tests/checker_tests.rs`)
19 comprehensive tests for business logic:

#### Own Table Access
- ✅ Extensions can access their own tables
- ✅ Table prefix format: `{public_key}__{extension_name}__{table_name}`

#### System Table Protection
- ✅ `haex_*` tables are always blocked
- ✅ `sqlite_master` and `sqlite_sequence` are blocked
- ✅ Wildcards do not grant system table access

#### Cross-Extension Access Control
- ✅ Extensions cannot access other extensions' tables without permission
- ✅ Prefix wildcard `other_key__other_ext__*` grants access to all tables of specific extension
- ✅ Full wildcard `*` grants access to all non-system tables

#### Permission Inheritance
- ✅ `ReadWrite` permission includes `Read`
- ✅ `Read` permission does not include `Write`

#### Wildcard Patterns
- ✅ Full wildcard: `*` matches all non-system tables
- ✅ Prefix wildcard: `prefix__*` matches tables starting with prefix
- ✅ Exact match: `exact_table` matches specific table only

#### Table Name Validation
- ✅ Own tables are always valid
- ✅ System tables are never valid
- ✅ Other tables require matching permission
- ✅ Handles quoted identifiers (double quotes and backticks)

### Layer 3: Utility Tests (`utils.rs`)
6 tests for table prefix utilities:
- ✅ Correct prefix generation
- ✅ Own table detection
- ✅ Other extension table detection
- ✅ System table detection
- ✅ Similar prefix handling (no false positives)
- ✅ Special characters in names

## Running Tests

```bash
# Run all extension tests
cargo test extension --lib

# Run all permission tests
cargo test extension::permissions::tests --lib

# Run only permission checker tests
cargo test extension::permissions::tests::checker_tests --lib

# Run only utility tests
cargo test extension::utils --lib

# Run only SQL parsing tests
cargo test extension::database::tests::sql_parsing_tests --lib
```

## Test Results

**Total: 61 extension tests, all passing**
- 19 permission logic tests
- 15 SQL parsing tests
- 6 utility tests
- 21 TypeScript export binding tests

## Security Guarantees

The test suite validates these critical security guarantees:

1. **Table Isolation**: Extensions can ONLY modify tables with their own prefix
2. **System Protection**: Core system tables are inaccessible to all extensions
3. **Cross-Extension Security**: Extensions cannot access other extensions' data without explicit permission
4. **Wildcard Safety**: Wildcard permissions never grant system table access
5. **Permission Precision**: Read/Write/Create permissions are properly scoped

## Future Work

While the current test suite is comprehensive for business logic, future enhancements could include:

1. **Integration Tests**: Testing the full command flow with a mock database
2. **Migration Tests**: Validating that extension schema migrations are properly isolated
3. **CRDT Sync Tests**: Ensuring permission checks work correctly during sync operations
4. **Performance Tests**: Benchmarking permission checks for large permission sets

## Architecture Benefits

This testing architecture provides several key benefits:

1. **Fast Tests**: No Tauri State mocking means tests run in milliseconds
2. **Maintainable**: Business logic is separated from framework dependencies
3. **Comprehensive**: 100% coverage of permission checking logic
4. **Debuggable**: Pure functions are easy to debug and reason about
5. **Refactorable**: Changes to permission logic only affect `PermissionChecker`
