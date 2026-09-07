import { sql } from 'drizzle-orm'
import {
  index,
  integer,
  sqliteTable,
  text,
  uniqueIndex,
} from 'drizzle-orm/sqlite-core'
import tableNames from '@/database/tableNames.json'

// ---------------------------------------------------------------------------
// File Grants — Phase 4 Round F1
// ---------------------------------------------------------------------------
//
// One row per (content_key, space_id) triple. Records "content object X is
// shared with space Y" so space members see grants as soon as the owner's
// CRDT push arrives, without a bucket LIST. Space-scoped by `space_id`
// so a Space Alpha member's device never receives a grant row for
// Space Beta.
//
// `sidecar_key` is the AEAD-sealed content encryption key wrapped for the
// space's current MLS epoch key. Members decrypt using the epoch key
// flowing through haex_mls_sync_keys.
//
// UNIQUE (content_key, space_id) forbids two active grants for the same
// content into the same space; the CRDT delete-log removes stale rows on
// unshare.
// ---------------------------------------------------------------------------

export const haexFileGrants = sqliteTable(
  tableNames.haex.file_grants.name,
  {
    id: text(tableNames.haex.file_grants.columns.id).primaryKey().notNull(),
    contentKey: text(tableNames.haex.file_grants.columns.contentKey).notNull(),
    spaceId: text(tableNames.haex.file_grants.columns.spaceId).notNull(),
    sidecarKey: text(tableNames.haex.file_grants.columns.sidecarKey).notNull(),
    epoch: integer(tableNames.haex.file_grants.columns.epoch).notNull(),
    createdAt: text(tableNames.haex.file_grants.columns.createdAt)
      .default(sql`(CURRENT_TIMESTAMP)`)
      .notNull(),
  },
  (table) => [
    uniqueIndex('haex_file_grants_content_space_uniq').on(
      table.contentKey,
      table.spaceId,
    ),
    index('haex_file_grants_space_idx').on(table.spaceId),
  ],
)
export type InsertHaexFileGrants = typeof haexFileGrants.$inferInsert
export type SelectHaexFileGrants = typeof haexFileGrants.$inferSelect

// ---------------------------------------------------------------------------
// S3 Shared Access — Phase 4 Round F1
// ---------------------------------------------------------------------------
//
// One row per (space_id, backend_id, member_did) triple. Carries an AEAD-
// sealed ScopedCred payload plus the epoch it was sealed under. Any current
// member can decrypt via the space epoch key flowing through
// haex_mls_sync_keys; a member kicked from the space keeps historical epoch
// keys but future rows are minted under a new epoch they no longer have —
// the encryption layer half of the revocation story (the other half is the
// IAM-provider-side rotation).
//
// UNIQUE (space_id, backend_id, member_did) — one active ScopedCred per
// (space, backend, member); re-provisioning overwrites via ON CONFLICT.
// ---------------------------------------------------------------------------

export const haexS3SharedAccess = sqliteTable(
  tableNames.haex.s3_shared_access.name,
  {
    id: text(tableNames.haex.s3_shared_access.columns.id).primaryKey().notNull(),
    spaceId: text(tableNames.haex.s3_shared_access.columns.spaceId).notNull(),
    backendId: text(tableNames.haex.s3_shared_access.columns.backendId).notNull(),
    memberDid: text(tableNames.haex.s3_shared_access.columns.memberDid).notNull(),
    encryptedCred: text(tableNames.haex.s3_shared_access.columns.encryptedCred).notNull(),
    epoch: integer(tableNames.haex.s3_shared_access.columns.epoch).notNull(),
    expiresAt: text(tableNames.haex.s3_shared_access.columns.expiresAt),
    createdAt: text(tableNames.haex.s3_shared_access.columns.createdAt)
      .default(sql`(CURRENT_TIMESTAMP)`)
      .notNull(),
  },
  (table) => [
    uniqueIndex('haex_s3_shared_access_space_backend_did_uniq').on(
      table.spaceId,
      table.backendId,
      table.memberDid,
    ),
    index('haex_s3_shared_access_member_idx').on(table.memberDid),
  ],
)
export type InsertHaexS3SharedAccess = typeof haexS3SharedAccess.$inferInsert
export type SelectHaexS3SharedAccess = typeof haexS3SharedAccess.$inferSelect
