import { sql } from 'drizzle-orm'
import { check, index, sqliteTable, text, uniqueIndex } from 'drizzle-orm/sqlite-core'
import tableNames from '@/database/tableNames.json'
import { haexSpaces } from './spaces'

// ---------------------------------------------------------------------------
// Space UCAN Grants — local, non-CRDT-synced bilateral UCAN storage: tracks
// both the grants this device issued to other space members and the grants
// it received from them. `_no_sync`: never touched by CRDT machinery, no
// haex_hlc / haex_column_hlcs / haex_column_sigs meta columns.
// See also: haexUcanTokens (CRDT-synced, different purpose — cached
// capability tokens for space operations, not bilateral grant bookkeeping).
// ---------------------------------------------------------------------------

export const haexSpaceUcanGrants = sqliteTable(
  tableNames.haex.space_ucan_grants_no_sync.name,
  {
    id: text(tableNames.haex.space_ucan_grants_no_sync.columns.id).primaryKey(),
    spaceId: text(tableNames.haex.space_ucan_grants_no_sync.columns.spaceId)
      .notNull()
      .references(() => haexSpaces.id, { onDelete: 'cascade' }),
    issuerDid: text(tableNames.haex.space_ucan_grants_no_sync.columns.issuerDid).notNull(),
    audienceDid: text(tableNames.haex.space_ucan_grants_no_sync.columns.audienceDid).notNull(),
    ucanToken: text(tableNames.haex.space_ucan_grants_no_sync.columns.ucanToken).notNull(),
    role: text(tableNames.haex.space_ucan_grants_no_sync.columns.role).notNull(), // 'issued' | 'received'
    createdAt: text(tableNames.haex.space_ucan_grants_no_sync.columns.createdAt).notNull(),
    revokedAt: text(tableNames.haex.space_ucan_grants_no_sync.columns.revokedAt),
  },
  (table) => [
    // Hot path is "active grants for (space, audience)" — a partial index
    // keeps it smaller than an unfiltered one and drops rows on revoke.
    index('haex_space_ucan_grants_active_lookup')
      .on(table.spaceId, table.audienceDid)
      .where(sql`${table.revokedAt} IS NULL`),
    // At most one ACTIVE grant per (space, issuer, audience, role); revoked
    // grants accumulate as history and are excluded from the constraint.
    uniqueIndex('haex_space_ucan_grants_active_uniq')
      .on(table.spaceId, table.issuerDid, table.audienceDid, table.role)
      .where(sql`${table.revokedAt} IS NULL`),
    check('haex_space_ucan_grants_no_sync_role_check', sql`role IN ('issued','received')`),
  ],
)
export type InsertHaexSpaceUcanGrants = typeof haexSpaceUcanGrants.$inferInsert
export type SelectHaexSpaceUcanGrants = typeof haexSpaceUcanGrants.$inferSelect
