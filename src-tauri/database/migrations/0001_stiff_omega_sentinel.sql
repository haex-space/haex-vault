CREATE TABLE `haex_logs` (
	`id` text PRIMARY KEY NOT NULL,
	`timestamp` text NOT NULL,
	`level` text NOT NULL,
	`source` text NOT NULL,
	`extension_id` text,
	`message` text NOT NULL,
	`metadata` text,
	`device_id` text NOT NULL,
	FOREIGN KEY (`extension_id`) REFERENCES `haex_extensions`(`id`) ON UPDATE no action ON DELETE cascade
);
--> statement-breakpoint
DROP INDEX `haex_vault_settings_key_type_unique`;--> statement-breakpoint
ALTER TABLE `haex_vault_settings` ADD `extension_id` text REFERENCES haex_extensions(id);--> statement-breakpoint
CREATE UNIQUE INDEX `haex_vault_settings_key_type_ext_unique` ON `haex_vault_settings` (`key`,`type`,`extension_id`);