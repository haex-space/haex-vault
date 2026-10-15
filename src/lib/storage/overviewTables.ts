/**
 * The tables whose changes (also from another device) change the storage overview of the settings
 * (`useStorageOverview`, spec 038, spec 052 FR-010). The state of a connection's credentials comes
 * from the password manager: the entry's values (`haex_passwords_item_details`) and whether it is
 * in the trash, which is its folder (`haex_passwords_group_items`) or a folder above it
 * (`haex_passwords_groups`). Without the folder tables, deleting the entry left the list saying
 * the credentials were present until it was opened again.
 */
export const OVERVIEW_TABLES = [
  'haex_storage_connections',
  'haex_storages',
  'storage_tests_no_sync',
  'haex_passwords_item_details',
  'haex_passwords_group_items',
  'haex_passwords_groups',
  'extension_permissions',
] as const
