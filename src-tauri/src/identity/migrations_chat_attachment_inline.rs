//! SQL of migration `0067_chat_attachment_inline` (spec 072 FR-012, research R2): a file of a chat
//! message can be the full text of the message, whose `text` is then only a preview.
//!
//! `chat_attachments` is tracked by haex-crdt, so the new column raises
//! [`super::migrations::HAEX_TRIGGER_VERSION`]. Every attachment from before is an ordinary file.

/// The migration `0067_chat_attachment_inline`; registered in
/// [`super::migrations::haex_migration_source`].
pub const CHAT_ATTACHMENT_INLINE_0067: &str =
    "ALTER TABLE chat_attachments ADD COLUMN inline INTEGER NOT NULL DEFAULT 0;";
