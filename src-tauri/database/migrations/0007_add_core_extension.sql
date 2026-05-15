INSERT OR IGNORE INTO `haex_extensions` (
	`id`,
	`public_key`,
	`name`,
	`version`,
	`signature`,
	`enabled`,
	`single_instance`,
	`display_mode`,
	`description`
) VALUES (
	'__core__',
	'__core__',
	'core',
	'0.0.0',
	'',
	true,
	false,
	'auto',
	'haex-vault built-in core feature target'
);
