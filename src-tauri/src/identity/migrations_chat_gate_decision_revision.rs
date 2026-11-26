//! SQL of migration `0072_chat_gate_decision_revision`: a synced per-peer answer decision and its
//! revision on the device-local notice and received-answer records.

/// The migration `0072_chat_gate_decision_revision`; registered in [`super::migrations::haex_migration_source`].
pub const CHAT_GATE_DECISION_REVISION_0072: &str = r#"CREATE TABLE haex_contacts_gate_decisions (
  peer_vault TEXT PRIMARY KEY NOT NULL,
  answer TEXT NOT NULL,
  revision TEXT NOT NULL
);
--> statement-breakpoint
ALTER TABLE chat_gate_notices_no_sync ADD COLUMN revision TEXT NOT NULL DEFAULT '';
--> statement-breakpoint
ALTER TABLE chat_gate_received_answers_no_sync ADD COLUMN revision TEXT NOT NULL DEFAULT '';"#;
