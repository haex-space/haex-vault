import { sql } from 'drizzle-orm'
import {
  sqliteTable,
  text,
  uniqueIndex,
} from 'drizzle-orm/sqlite-core'
import tableNames from '@/database/tableNames.json'

// ---------------------------------------------------------------------------
// Identities — user-managed keypairs for space authentication (did:key)
// ---------------------------------------------------------------------------

export const haexIdentities = sqliteTable(
  tableNames.haex.identities.name,
  {
    publicKey: text(tableNames.haex.identities.columns.publicKey)
      .notNull()
      .primaryKey(), // Base64 SPKI Ed25519 signing key — stable, unique, same across all devices
    label: text(tableNames.haex.identities.columns.label).notNull(),
    did: text(tableNames.haex.identities.columns.did).notNull(), // did:key:z6Mk...
    privateKey: text(tableNames.haex.identities.columns.privateKey).notNull(), // Base64 PKCS8 Ed25519 signing key (X25519 derived on-the-fly via Rust)
    avatar: text(tableNames.haex.identities.columns.avatar), // Base64 WebP 128x128
    createdAt: text(tableNames.haex.identities.columns.createdAt).default(
      sql`(CURRENT_TIMESTAMP)`,
    ),
  },
  (table) => [
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
      .references(() => haexIdentities.publicKey, { onDelete: 'cascade' }),
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

// ---------------------------------------------------------------------------
// Contacts — external people's public keys and metadata (address book)
// ---------------------------------------------------------------------------

export const haexContacts = sqliteTable(
  tableNames.haex.contacts.name,
  {
    id: text(tableNames.haex.contacts.columns.id)
      .$defaultFn(() => crypto.randomUUID())
      .primaryKey(),
    label: text(tableNames.haex.contacts.columns.label).notNull(),
    publicKey: text(tableNames.haex.contacts.columns.publicKey).notNull(),
    avatar: text(tableNames.haex.contacts.columns.avatar), // Base64 WebP 128x128
    notes: text(tableNames.haex.contacts.columns.notes),
    createdAt: text(tableNames.haex.contacts.columns.createdAt).default(
      sql`(CURRENT_TIMESTAMP)`,
    ),
  },
  (table) => [
    uniqueIndex('haex_contacts_public_key_unique').on(table.publicKey),
  ],
)
export type InsertHaexContacts = typeof haexContacts.$inferInsert
export type SelectHaexContacts = typeof haexContacts.$inferSelect

export const haexContactClaims = sqliteTable(
  tableNames.haex.contact_claims.name,
  {
    id: text(tableNames.haex.contact_claims.columns.id)
      .$defaultFn(() => crypto.randomUUID())
      .primaryKey(),
    contactId: text(tableNames.haex.contact_claims.columns.contactId)
      .notNull()
      .references(() => haexContacts.id, { onDelete: 'cascade' }),
    type: text(tableNames.haex.contact_claims.columns.type).notNull(),
    value: text(tableNames.haex.contact_claims.columns.value).notNull(),
    createdAt: text(tableNames.haex.contact_claims.columns.createdAt).default(
      sql`(CURRENT_TIMESTAMP)`,
    ),
  },
)
export type InsertHaexContactClaims = typeof haexContactClaims.$inferInsert
export type SelectHaexContactClaims = typeof haexContactClaims.$inferSelect
