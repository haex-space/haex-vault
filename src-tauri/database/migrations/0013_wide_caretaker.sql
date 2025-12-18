DROP INDEX `haex_file_local_sync_state_file_id_unique`;--> statement-breakpoint
DROP INDEX `haex_file_local_sync_state_local_path_unique`;--> statement-breakpoint
ALTER TABLE `haex_file_local_sync_state` ADD `device_id` text NOT NULL DEFAULT '';--> statement-breakpoint
CREATE UNIQUE INDEX `haex_file_local_sync_state_device_file_unique` ON `haex_file_local_sync_state` (`device_id`,`file_id`) WHERE "haex_file_local_sync_state"."haex_tombstone" = 0;--> statement-breakpoint
CREATE UNIQUE INDEX `haex_file_local_sync_state_device_path_unique` ON `haex_file_local_sync_state` (`device_id`,`local_path`) WHERE "haex_file_local_sync_state"."haex_tombstone" = 0;--> statement-breakpoint
CREATE INDEX `haex_file_local_sync_state_device_id_idx` ON `haex_file_local_sync_state` (`device_id`);--> statement-breakpoint
DROP INDEX `haex_file_sync_rules_local_path_unique`;--> statement-breakpoint
ALTER TABLE `haex_file_sync_rules` ADD `device_id` text NOT NULL DEFAULT '';--> statement-breakpoint
CREATE UNIQUE INDEX `haex_file_sync_rules_device_path_unique` ON `haex_file_sync_rules` (`device_id`,`local_path`) WHERE "haex_file_sync_rules"."haex_tombstone" = 0;--> statement-breakpoint
CREATE INDEX `haex_file_sync_rules_device_id_idx` ON `haex_file_sync_rules` (`device_id`);