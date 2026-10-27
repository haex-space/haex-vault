//! SQL of migration `0045_sync_rules_decrypt` (spec 061 FR-030a): whether the user agreed that a
//! rule stores files of an encrypted place decrypted at its other place. The table is `_no_sync`
//! (ADR 0018), so [`super::migrations::HOLZI_TRIGGER_VERSION`] stays.

/// The migration `0045_sync_rules_decrypt`; registered in
/// [`super::migrations::holzi_migration_source`].
pub const SYNC_RULES_DECRYPT_0045: &str =
    "ALTER TABLE haex_sync_rules_no_sync ADD COLUMN allow_decrypted INTEGER NOT NULL DEFAULT 0;";
