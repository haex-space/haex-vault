-- Add dev_path column to haex_extensions table
-- If dev_path is set, this is a dev extension; if NULL, this is a production extension
ALTER TABLE `haex_extensions` ADD `dev_path` text;
