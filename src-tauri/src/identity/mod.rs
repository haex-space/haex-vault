//! Provider seams haex-vault implements against `haex-crdt` 0.4.0.
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
mod migrations_agent_browser;
#[cfg(test)]
mod migrations_agent_browser_tests;
mod migrations_agent_files;
mod migrations_agent_memberships;
#[cfg(test)]
mod migrations_agent_memberships_tests;
mod migrations_agent_memory;
#[cfg(test)]
mod migrations_agent_memory_tests;
mod migrations_agent_profiles;
#[cfg(test)]
mod migrations_agent_profiles_tests;
mod migrations_assistant_speakers;
#[cfg(test)]
mod migrations_assistant_speakers_tests;
mod migrations_assistant_tones;
#[cfg(test)]
mod migrations_assistant_tones_tests;
mod migrations_browser_clients;
#[cfg(test)]
mod migrations_browser_clients_tests;
mod migrations_chat_attachment_inline;
#[cfg(test)]
mod migrations_chat_attachment_inline_tests;
mod migrations_chat_contacts;
#[cfg(test)]
mod migrations_chat_contacts_tests;
// The chat's erasure test (spec 095 T010) builds the tables on an unencrypted file.
mod migrations_chat_gate;
#[cfg(test)]
mod migrations_chat_gate_tests;
pub(crate) mod migrations_chat_ratchet;
#[cfg(test)]
mod migrations_chat_ratchet_tests;
mod migrations_chat_record_relays;
#[cfg(test)]
mod migrations_chat_record_relays_tests;
mod migrations_contacts;
mod migrations_contacts_cards;
#[cfg(test)]
mod migrations_contacts_cards_tests;
mod migrations_contacts_self;
#[cfg(test)]
mod migrations_contacts_self_tests;
#[cfg(test)]
mod migrations_contacts_tests;
mod migrations_default_model;
#[cfg(test)]
mod migrations_default_model_tests;
mod migrations_diagnostic_log;
#[cfg(test)]
mod migrations_diagnostic_log_tests;
mod migrations_drop_agent_files;
#[cfg(test)]
mod migrations_drop_agent_files_tests;
mod migrations_drop_agent_grants;
#[cfg(test)]
mod migrations_drop_agent_grants_tests;
pub mod migrations_encrypted_folders;
#[cfg(test)]
mod migrations_encrypted_folders_tests;
pub mod migrations_extensions;
#[cfg(test)]
mod migrations_extensions_tests;
mod migrations_file_shares;
#[cfg(test)]
mod migrations_file_shares_tests;
mod migrations_haex_names;
#[cfg(test)]
mod migrations_haex_names_tests;
mod migrations_icon_sets;
#[cfg(test)]
mod migrations_icon_sets_tests;
mod migrations_legacy_names;
#[cfg(test)]
mod migrations_legacy_names_tests;
mod migrations_marketplaces;
#[cfg(test)]
mod migrations_marketplaces_tests;
mod migrations_mcp_agents;
#[cfg(test)]
mod migrations_mcp_agents_tests;
mod migrations_model_access;
#[cfg(test)]
mod migrations_model_access_tests;
mod migrations_passwords;
mod migrations_passwords_owner;
#[cfg(test)]
mod migrations_passwords_owner_tests;
mod migrations_passwords_pass;
#[cfg(test)]
mod migrations_passwords_pass_tests;
mod migrations_passwords_refs;
#[cfg(test)]
mod migrations_passwords_refs_tests;
mod migrations_peer_schema;
#[cfg(test)]
mod migrations_peer_schema_tests;
pub mod migrations_storage;
#[cfg(test)]
mod migrations_storage_tests;
mod migrations_sync_rules;
mod migrations_sync_rules_decrypt;
mod migrations_sync_rules_source_kind;
#[cfg(test)]
mod migrations_sync_rules_source_kind_tests;
mod migrations_sync_rules_source_target;
#[cfg(test)]
mod migrations_sync_rules_source_target_tests;
#[cfg(test)]
mod migrations_sync_rules_tests;
mod migrations_sync_runs_timing;
#[cfg(test)]
mod migrations_sync_runs_timing_tests;
#[cfg(test)]
mod migrations_tests;
mod migrations_tts_voices;
#[cfg(test)]
mod migrations_tts_voices_tests;
mod migrations_voice;
#[cfg(test)]
mod migrations_voice_tests;

pub use bootstrap::HaexBootstrap;
pub use installation::{installation_id_path, read_or_mint_installation_uuid};
pub use migrations::{haex_migration_source, HAEX_TRIGGER_VERSION};
pub use migrations_legacy_names::{
    haex_migration_source_with_legacy_names, is_legacy_migration_name,
};

/// Nil UUID reserved as the sentinel row in `known_devices` representing
/// vault-scope for tables with a device-discriminating primary key. See
/// ADR-0001 (`docs/adr/0001-device-scoped-data-convention.md`).
pub const VAULT_SCOPE_UUID: uuid::Uuid = uuid::Uuid::nil();
