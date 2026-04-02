import { sql } from 'drizzle-orm'
import {
  sqliteTable,
  text,
  uniqueIndex,
} from 'drizzle-orm/sqlite-core'
import tableNames from '@/database/tableNames.json'

// ---------------------------------------------------------------------------
// Identities — unified table for own keypairs AND external contacts
// Own identity: privateKey is set. Contact: privateKey is null.
// ---------------------------------------------------------------------------

export const haexIdentities = sqliteTable(
  tableNames.haex.identities.name,
  {
    id: text(tableNames.haex.identities.columns.id)
      .$defaultFn(() => crypto.randomUUID())
      .primaryKey(),
    publicKey: text(tableNames.haex.identities.columns.publicKey).notNull(),
    did: text(tableNames.haex.identities.columns.did).notNull(),
    label: text(tableNames.haex.identities.columns.label).notNull(),
    privateKey: text(tableNames.haex.identities.columns.privateKey), // null = contact, non-null = own identity
    avatar: text(tableNames.haex.identities.columns.avatar), // Base64 WebP 128x128
    notes: text(tableNames.haex.identities.columns.notes),
    createdAt: text(tableNames.haex.identities.columns.createdAt).default(
      sql`(CURRENT_TIMESTAMP)`,
    ),
  },
  (table) => [
    uniqueIndex('haex_identities_public_key_unique').on(table.publicKey),
    uniqueIndex('haex_identities_did_unique').on(table.did),
  ],
)
export type InsertHaexIdentities = typeof haexIdentities.$inferInsert
export type SelectHaexIdentities = typeof haexIdentities.$inferSelect

// ---------------------------------------------------------------------------
// Identity Claims — attributes for selective disclosure to servers
// ---------------------------------------------------------------------------

export const haexIdentityClaims = sqliteTable(
  tableNames.haex.identity_claims.name,
  {
    id: text(tableNames.haex.identity_claims.columns.id)
      .$defaultFn(() => crypto.randomUUID())
      .primaryKey(),
    identityId: text(tableNames.haex.identity_claims.columns.identityId)
      .notNull()
      .references(() => haexIdentities.id, { onDelete: 'cascade' }),
    type: text(tableNames.haex.identity_claims.columns.type).notNull(),
    value: text(tableNames.haex.identity_claims.columns.value).notNull(),
    verifiedAt: text(tableNames.haex.identity_claims.columns.verifiedAt),
    verifiedBy: text(tableNames.haex.identity_claims.columns.verifiedBy),
    createdAt: text(tableNames.haex.identity_claims.columns.createdAt).default(
      sql`(CURRENT_TIMESTAMP)`,
    ),
  },
)
export type InsertHaexIdentityClaims = typeof haexIdentityClaims.$inferInsert
export type SelectHaexIdentityClaims = typeof haexIdentityClaims.$inferSelect

