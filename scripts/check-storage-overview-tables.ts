// Part of `pnpm check:storage` (spec 052 FR-010): the tables the storage overview follows
// (src/lib/storage/overviewTables.ts) include those its credentials state is read from, the
// entry's values and the folders that say whether it is in the trash. e2e
// `storage-credentials-entry` saw the list keep "present" after the entry was deleted.
import assert from 'node:assert/strict'
import { test } from 'node:test'

import { OVERVIEW_TABLES } from '../src/lib/storage/overviewTables.ts'

test('the overview follows the entry of its credentials, also into the trash', () => {
  for (const table of [
    'haex_passwords_item_details',
    'haex_passwords_group_items',
    'haex_passwords_groups',
  ])
    assert.ok(OVERVIEW_TABLES.includes(table as never), table)
})

test('the overview follows its own tables', () => {
  for (const table of [
    'haex_storage_connections',
    'haex_storages',
    'storage_tests_no_sync',
  ])
    assert.ok(OVERVIEW_TABLES.includes(table as never), table)
})
