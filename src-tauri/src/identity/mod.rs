//! Provider seams holzi implements against `haex-crdt` 0.4.0.
//!
//! Contract: [`specs/001-frontend-onboarding/contracts/tauri-commands.md`](../../../../specs/001-frontend-onboarding/contracts/tauri-commands.md)
//! §"Vault identity and device model" and §"Provider implementations".
//!
//! Extraction into a standalone `haex-identity` crate stays deferred until
//! the trait surface has stabilised against a working slice (per plan §"Architektur
//! und Verantwortlichkeiten"). For now it lives as one Cargo module inside
//! `src-tauri`.

pub mod bootstrap;
pub mod installation;
pub mod migrations;
mod migrations_agent_files;
#[cfg(test)]
mod migrations_agent_files_tests;
mod migrations_browser_clients;
#[cfg(test)]
mod migrations_browser_clients_tests;
mod migrations_contacts;
#[cfg(test)]
mod migrations_contacts_tests;
pub mod migrations_encrypted_folders;
#[cfg(test)]
mod migrations_encrypted_folders_tests;
pub mod migrations_extensions;
#[cfg(test)]
mod migrations_extensions_tests;
mod migrations_file_shares;
#[cfg(test)]
mod migrations_file_shares_tests;
mod migrations_icon_sets;
#[cfg(test)]
mod migrations_icon_sets_tests;
mod migrations_model_access;
#[cfg(test)]
mod migrations_model_access_tests;
mod migrations_passwords;
mod migrations_passwords_owner;
#[cfg(test)]
mod migrations_passwords_owner_tests;
mod migrations_passwords_refs;
#[cfg(test)]
mod migrations_passwords_refs_tests;
mod migrations_peer_schema;
#[cfg(test)]
mod migrations_peer_schema_tests;
pub mod migrations_storage;
#[cfg(test)]
mod migrations_storage_tests;
#[cfg(test)]
mod migrations_tests;

pub use bootstrap::HolziBootstrap;
pub use installation::{installation_id_path, read_or_mint_installation_uuid};
pub use migrations::{
    holzi_migration_source, holzi_migration_source_with_legacy_contacts_display_name,
    holzi_migration_source_with_legacy_icon_set_name, HOLZI_TRIGGER_VERSION,
};

/// Nil UUID reserved as the sentinel row in `known_devices` representing
/// vault-scope for tables with a device-discriminating primary key. See
/// ADR-0001 (`docs/adr/0001-device-scoped-data-convention.md`).
pub const VAULT_SCOPE_UUID: uuid::Uuid = uuid::Uuid::nil();
