DROP INDEX `haex_crdt_migrations_migration_name_unique`;--> statement-breakpoint
CREATE UNIQUE INDEX `haex_crdt_migrations_ext_name_unique` ON `haex_crdt_migrations` (`extension_id`,`migration_name`);