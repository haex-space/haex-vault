ALTER TABLE `haex_extension_permissions` RENAME TO `haex_principal_permissions`;--> statement-breakpoint
ALTER TABLE `haex_principal_permissions` RENAME COLUMN "extension_id" TO "principal_id";--> statement-breakpoint
PRAGMA foreign_keys=OFF;--> statement-breakpoint
CREATE TABLE `__new_haex_principal_permissions` (
	`id` text PRIMARY KEY NOT NULL,
	`principal_id` text NOT NULL,
	`resource_type` text,
	`action` text,
	`target` text,
	`constraints` text,
	`status` text DEFAULT 'denied' NOT NULL,
	`created_at` text DEFAULT (CURRENT_TIMESTAMP),
	`updated_at` integer,
	FOREIGN KEY (`principal_id`) REFERENCES `haex_extensions`(`id`) ON UPDATE no action ON DELETE cascade
);
--> statement-breakpoint
INSERT INTO `__new_haex_principal_permissions`("id", "principal_id", "resource_type", "action", "target", "constraints", "status", "created_at", "updated_at") SELECT "id", "principal_id", "resource_type", "action", "target", "constraints", "status", "created_at", "updated_at" FROM `haex_principal_permissions`;--> statement-breakpoint
DROP TABLE `haex_principal_permissions`;--> statement-breakpoint
ALTER TABLE `__new_haex_principal_permissions` RENAME TO `haex_principal_permissions`;--> statement-breakpoint
PRAGMA foreign_keys=ON;--> statement-breakpoint
CREATE UNIQUE INDEX `haex_principal_permissions_principal_id_resource_type_action_target_unique` ON `haex_principal_permissions` (`principal_id`,`resource_type`,`action`,`target`);