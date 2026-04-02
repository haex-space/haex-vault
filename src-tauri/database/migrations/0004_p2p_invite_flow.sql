-- Migration: P2P invite flow schema changes
-- 1. haex_spaces: rename type 'shared' → 'online', add status column, rename server_url → origin_url
-- 2. haex_pending_invites: remove space_name/capability, add capabilities/token_id/space_endpoints
-- 3. New tables: haex_invite_outbox, haex_invite_tokens (CRDT-synced)

-- 1. Rename space type 'shared' → 'online'
UPDATE haex_spaces SET type = 'online' WHERE type = 'shared';
--> statement-breakpoint
-- 2. Add status column to haex_spaces
ALTER TABLE `haex_spaces` ADD `status` text DEFAULT 'active' NOT NULL;
--> statement-breakpoint
-- 3. Rename server_url → origin_url
ALTER TABLE `haex_spaces` RENAME COLUMN `server_url` TO `origin_url`;
--> statement-breakpoint
-- 4. Recreate haex_pending_invites to remove space_name/capability, add new columns.
-- Use _no_sync suffix so CrdtTransformer does NOT add CRDT columns (we add them manually
-- to preserve existing CRDT data during the copy). After rename, triggers will be set up
-- by ensure_triggers_for_all_tables() since the final table has haex_tombstone.
CREATE TABLE `haex_pending_invites_temp_no_sync` (
	`id` text PRIMARY KEY NOT NULL,
	`space_id` text NOT NULL,
	`inviter_did` text NOT NULL,
	`inviter_label` text,
	`capabilities` text,
	`include_history` integer DEFAULT false,
	`token_id` text,
	`space_endpoints` text,
	`status` text DEFAULT 'pending' NOT NULL,
	`created_at` text NOT NULL,
	`responded_at` text,
	`haex_timestamp` text,
	`haex_column_hlcs` text,
	`haex_tombstone` integer,
	FOREIGN KEY (`space_id`) REFERENCES `haex_spaces`(`id`) ON UPDATE no action ON DELETE cascade
);
--> statement-breakpoint
-- 5. Copy data from old table to new, converting capability → capabilities JSON array
INSERT INTO haex_pending_invites_temp_no_sync (id, space_id, inviter_did, inviter_label, capabilities, include_history, status, created_at, responded_at, haex_timestamp, haex_column_hlcs, haex_tombstone)
	SELECT id, space_id, inviter_did, inviter_label,
		CASE WHEN capability IS NOT NULL THEN '["' || capability || '"]' ELSE '["space/read"]' END,
		include_history, status, created_at, responded_at,
		haex_timestamp, haex_column_hlcs, haex_tombstone
	FROM haex_pending_invites;
--> statement-breakpoint
-- 6. Drop old table
DROP TABLE haex_pending_invites;
--> statement-breakpoint
-- 7. Rename temp table to final name
ALTER TABLE `haex_pending_invites_temp_no_sync` RENAME TO `haex_pending_invites`;
--> statement-breakpoint
-- 8. Create invite outbox (CRDT-synced — CrdtTransformer adds CRDT columns automatically)
CREATE TABLE `haex_invite_outbox` (
	`id` text PRIMARY KEY NOT NULL,
	`space_id` text NOT NULL,
	`token_id` text NOT NULL,
	`target_did` text NOT NULL,
	`target_endpoint_id` text NOT NULL,
	`status` text DEFAULT 'pending' NOT NULL,
	`retry_count` integer DEFAULT 0 NOT NULL,
	`next_retry_at` text NOT NULL,
	`expires_at` text NOT NULL,
	`created_at` text NOT NULL
);
--> statement-breakpoint
-- 9. Create invite tokens (CRDT-synced — CrdtTransformer adds CRDT columns automatically)
CREATE TABLE `haex_invite_tokens` (
	`id` text PRIMARY KEY NOT NULL,
	`space_id` text NOT NULL,
	`target_did` text,
	`capabilities` text,
	`pre_created_ucan` text,
	`include_history` integer DEFAULT false,
	`max_uses` integer DEFAULT 1 NOT NULL,
	`current_uses` integer DEFAULT 0 NOT NULL,
	`expires_at` text NOT NULL,
	`created_at` text NOT NULL
);
