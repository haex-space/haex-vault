-- Phase 4 Round C: opaque cloud object-key mapping for `file_sync`.
--
-- `object_key` caches the random bucket object key a cloud sync target's
-- content was sealed under (see `file_sync::crypto::object_key`), so the
-- diff/execute path never has to re-derive it. Nullable: rows from
-- non-cloud targets and rows written before this column existed carry no
-- object key.
ALTER TABLE `haex_sync_state_no_sync` ADD `object_key` text;
--> statement-breakpoint
CREATE INDEX `haex_sync_state_object_key_idx` ON `haex_sync_state_no_sync` (`object_key`);
