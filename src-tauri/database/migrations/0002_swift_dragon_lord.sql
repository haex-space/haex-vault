DROP INDEX `haex_principals_public_key_kind_unique`;--> statement-breakpoint
CREATE UNIQUE INDEX `haex_principals_public_key_kind_name_unique` ON `haex_principals` (`public_key`,`kind`,`name`);