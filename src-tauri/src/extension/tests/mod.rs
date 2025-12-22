// src-tauri/src/extension/tests/mod.rs
//!
//! Integration tests for extension API commands
//!
//! These tests verify the extension command behavior including:
//! - Request/Response validation
//! - Permission checks
//! - Error handling
//! - Security against malicious inputs
//!

#[cfg(test)]
mod command_validation_tests;
#[cfg(test)]
mod request_types_tests;
#[cfg(test)]
mod security_tests;
