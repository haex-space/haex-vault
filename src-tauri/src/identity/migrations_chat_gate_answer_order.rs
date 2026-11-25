//! SQL of migration `0071_chat_gate_answer_order`: monotonic per-peer answer counters on the
//! answering device, plus the last counter observed for each sender device.

/// The migration `0071_chat_gate_answer_order`; registered in [`super::migrations::haex_migration_source`].
pub const CHAT_GATE_ANSWER_ORDER_0071: &str = r#"ALTER TABLE chat_gate_notices_no_sync ADD COLUMN sequence INTEGER NOT NULL DEFAULT 1;
--> statement-breakpoint
CREATE TABLE chat_gate_answer_sequences_no_sync (
  peer_vault TEXT PRIMARY KEY NOT NULL,
  sequence INTEGER NOT NULL
);
--> statement-breakpoint
INSERT INTO chat_gate_answer_sequences_no_sync (peer_vault, sequence)
SELECT peer_vault, sequence FROM chat_gate_notices_no_sync;
--> statement-breakpoint
CREATE TABLE chat_gate_received_answers_no_sync (
  peer_vault TEXT NOT NULL,
  sender_device TEXT NOT NULL,
  sequence INTEGER NOT NULL,
  PRIMARY KEY (peer_vault, sender_device)
);"#;
