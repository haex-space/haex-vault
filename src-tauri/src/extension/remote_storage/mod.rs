// src-tauri/src/extension/remote_storage/mod.rs
//!
//! Extension Remote Storage Module
//!
//! This module provides permission-checked access to the internal storage API.
//! Extensions call these commands through the SDK, and we validate permissions
//! before delegating to the internal storage functions.
//!

pub mod commands;
