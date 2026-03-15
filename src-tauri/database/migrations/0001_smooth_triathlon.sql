CREATE TABLE `haex_logs` (
	`id` text PRIMARY KEY NOT NULL,
	`timestamp` text NOT NULL,
	`level` text NOT NULL,
	`source` text NOT NULL,
	`source_type` text NOT NULL,
	`message` text NOT NULL,
	`metadata` text,
	`device_id` text NOT NULL
);
--> statement-breakpoint
DROP INDEX `haex_vault_settings_key_type_unique`;--> statement-breakpoint
ALTER TABLE `haex_vault_settings` ADD `extension_id` text REFERENCES haex_extensions(id);--> statement-breakpoint
CREATE UNIQUE INDEX `haex_vault_settings_key_type_ext_unique` ON `haex_vault_settings` (`key`,`type`,`extension_id`);