DROP TABLE IF EXISTS `haex_logs`;
--> statement-breakpoint
CREATE TABLE `haex_logs_no_sync` (
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
