import { sql } from 'drizzle-orm'
import {
  integer,
  sqliteTable,
  text,
  uniqueIndex,
  type AnySQLiteColumn,
} from 'drizzle-orm/sqlite-core'
import tableNames from '@/database/tableNames.json'

// ---------------------------------------------------------------------------
// Passwords — absorbed from the haex-pass extension into core (2026-04-19).
//
// Design decisions:
//   1. No API-level type variant — one unified "password" with optional
//      fields (password / OTP / passkey / attachments / custom fields).
//   2. Relations stay normalized — tags, key-values, binaries, passkeys,
//      snapshots live in their own tables so CRDT can merge per-row and
//      unique constraints (tag.name, passkey.credential_id, binary.hash)
//      are enforceable.
//   3. No second encryption envelope — vault DB is encrypted at rest.
//   4. Sharing exclusively via the existing Spaces mechanism
//      (haex_shared_space_sync). No per-secret UCAN.
//   5. Groups = UI-only folders (hierarchical, 1 item -> 1 group).
//      Tags = logical classification (1 item -> n tags), drive extension
//      permission scoping via the permission.target field.
// ---------------------------------------------------------------------------

// Core password entry. OTP fields are inline (1:1). All other relations are
// separate tables below.
export const haexPasswordsItemDetails = sqliteTable(
  tableNames.haex.passwords_item_details.name,
  {
    id: text(tableNames.haex.passwords_item_details.columns.id)
      .$defaultFn(() => crypto.randomUUID())
      .primaryKey(),
    title: text(tableNames.haex.passwords_item_details.columns.title),
    username: text(tableNames.haex.passwords_item_details.columns.username),
    password: text(tableNames.haex.passwords_item_details.columns.password),
    note: text(tableNames.haex.passwords_item_details.columns.note),
    icon: text(tableNames.haex.passwords_item_details.columns.icon),
    color: text(tableNames.haex.passwords_item_details.columns.color),
    url: text(tableNames.haex.passwords_item_details.columns.url),
    otpSecret: text(tableNames.haex.passwords_item_details.columns.otpSecret),
    otpDigits: integer(tableNames.haex.passwords_item_details.columns.otpDigits).default(6),
    otpPeriod: integer(tableNames.haex.passwords_item_details.columns.otpPeriod).default(30),
    otpAlgorithm: text(tableNames.haex.passwords_item_details.columns.otpAlgorithm).default('SHA1'),
    expiresAt: text(tableNames.haex.passwords_item_details.columns.expiresAt),
    // JSON: maps canonical field names to autofill aliases for browser matching.
    // e.g. { "username": ["email", "login"], "password": ["pass"] }
    autofillAliases: text(tableNames.haex.passwords_item_details.columns.autofillAliases, {
      mode: 'json',
    }).$type<Record<string, string[]>>(),
    createdAt: text(tableNames.haex.passwords_item_details.columns.createdAt).default(
      sql`(CURRENT_TIMESTAMP)`,
    ),
    updatedAt: text(tableNames.haex.passwords_item_details.columns.updatedAt).default(
      sql`(CURRENT_TIMESTAMP)`,
    ),
  },
)
export type InsertHaexPasswordsItemDetails = typeof haexPasswordsItemDetails.$inferInsert
export type SelectHaexPasswordsItemDetails = typeof haexPasswordsItemDetails.$inferSelect

// User-defined extra fields on an item (e.g. "Recovery Code", "PIN").
export const haexPasswordsItemKeyValues = sqliteTable(
  tableNames.haex.passwords_item_key_values.name,
  {
    id: text(tableNames.haex.passwords_item_key_values.columns.id)
      .$defaultFn(() => crypto.randomUUID())
      .primaryKey(),
    itemId: text(tableNames.haex.passwords_item_key_values.columns.itemId)
      .notNull()
      .references((): AnySQLiteColumn => haexPasswordsItemDetails.id, { onDelete: 'cascade' }),
    key: text(tableNames.haex.passwords_item_key_values.columns.key),
    value: text(tableNames.haex.passwords_item_key_values.columns.value),
    updatedAt: text(tableNames.haex.passwords_item_key_values.columns.updatedAt).default(
      sql`(CURRENT_TIMESTAMP)`,
    ),
  },
)
export type InsertHaexPasswordsItemKeyValues = typeof haexPasswordsItemKeyValues.$inferInsert
export type SelectHaexPasswordsItemKeyValues = typeof haexPasswordsItemKeyValues.$inferSelect

// UI-only hierarchical folders.
export const haexPasswordsGroups = sqliteTable(
  tableNames.haex.passwords_groups.name,
  {
    id: text(tableNames.haex.passwords_groups.columns.id)
      .$defaultFn(() => crypto.randomUUID())
      .primaryKey(),
    name: text(tableNames.haex.passwords_groups.columns.name),
    description: text(tableNames.haex.passwords_groups.columns.description),
    icon: text(tableNames.haex.passwords_groups.columns.icon),
    sortOrder: integer(tableNames.haex.passwords_groups.columns.sortOrder),
    color: text(tableNames.haex.passwords_groups.columns.color),
    parentId: text(tableNames.haex.passwords_groups.columns.parentId).references(
      (): AnySQLiteColumn => haexPasswordsGroups.id,
      { onDelete: 'cascade' },
    ),
    createdAt: text(tableNames.haex.passwords_groups.columns.createdAt).default(
      sql`(CURRENT_TIMESTAMP)`,
    ),
    updatedAt: text(tableNames.haex.passwords_groups.columns.updatedAt).default(
      sql`(CURRENT_TIMESTAMP)`,
    ),
  },
)
export type InsertHaexPasswordsGroups = typeof haexPasswordsGroups.$inferInsert
export type SelectHaexPasswordsGroups = typeof haexPasswordsGroups.$inferSelect

// Item <-> Group: 1:1 per item (itemId is PK, so a change to group_id is
// tracked by CRDT as an update on a single row).
export const haexPasswordsGroupItems = sqliteTable(
  tableNames.haex.passwords_group_items.name,
  {
    itemId: text(tableNames.haex.passwords_group_items.columns.itemId)
      .primaryKey()
      .references((): AnySQLiteColumn => haexPasswordsItemDetails.id, { onDelete: 'cascade' }),
    groupId: text(tableNames.haex.passwords_group_items.columns.groupId).references(
      (): AnySQLiteColumn => haexPasswordsGroups.id,
      { onDelete: 'cascade' },
    ),
  },
)
export type InsertHaexPasswordsGroupItems = typeof haexPasswordsGroupItems.$inferInsert
export type SelectHaexPasswordsGroupItems = typeof haexPasswordsGroupItems.$inferSelect

// SHA-256 hash as PK deduplicates identical blobs across items and across
// devices — two devices generating the same attachment produce the same row.
export const haexPasswordsBinaries = sqliteTable(
  tableNames.haex.passwords_binaries.name,
  {
    hash: text(tableNames.haex.passwords_binaries.columns.hash).primaryKey(),
    data: text(tableNames.haex.passwords_binaries.columns.data).notNull(),
    size: integer(tableNames.haex.passwords_binaries.columns.size).notNull(),
    type: text(tableNames.haex.passwords_binaries.columns.type).default('attachment'),
    createdAt: text(tableNames.haex.passwords_binaries.columns.createdAt).default(
      sql`(CURRENT_TIMESTAMP)`,
    ),
  },
)
export type InsertHaexPasswordsBinaries = typeof haexPasswordsBinaries.$inferInsert
export type SelectHaexPasswordsBinaries = typeof haexPasswordsBinaries.$inferSelect

// n:m junction; file name may differ per-item for a shared binary.
export const haexPasswordsItemBinaries = sqliteTable(
  tableNames.haex.passwords_item_binaries.name,
  {
    id: text(tableNames.haex.passwords_item_binaries.columns.id)
      .$defaultFn(() => crypto.randomUUID())
      .primaryKey(),
    itemId: text(tableNames.haex.passwords_item_binaries.columns.itemId)
      .notNull()
      .references((): AnySQLiteColumn => haexPasswordsItemDetails.id, { onDelete: 'cascade' }),
    binaryHash: text(tableNames.haex.passwords_item_binaries.columns.binaryHash)
      .notNull()
      .references((): AnySQLiteColumn => haexPasswordsBinaries.hash, { onDelete: 'cascade' }),
    fileName: text(tableNames.haex.passwords_item_binaries.columns.fileName).notNull(),
  },
)
export type InsertHaexPasswordsItemBinaries = typeof haexPasswordsItemBinaries.$inferInsert
export type SelectHaexPasswordsItemBinaries = typeof haexPasswordsItemBinaries.$inferSelect

// Point-in-time snapshots. snapshotData is JSON so the history format stays
// stable if the items schema evolves later (event-sourcing pattern).
export const haexPasswordsItemSnapshots = sqliteTable(
  tableNames.haex.passwords_item_snapshots.name,
  {
    id: text(tableNames.haex.passwords_item_snapshots.columns.id)
      .$defaultFn(() => crypto.randomUUID())
      .primaryKey(),
    itemId: text(tableNames.haex.passwords_item_snapshots.columns.itemId)
      .notNull()
      .references((): AnySQLiteColumn => haexPasswordsItemDetails.id, { onDelete: 'cascade' }),
    snapshotData: text(tableNames.haex.passwords_item_snapshots.columns.snapshotData).notNull(),
    createdAt: text(tableNames.haex.passwords_item_snapshots.columns.createdAt).default(
      sql`(CURRENT_TIMESTAMP)`,
    ),
    modifiedAt: text(tableNames.haex.passwords_item_snapshots.columns.modifiedAt),
  },
)
export type InsertHaexPasswordsItemSnapshots = typeof haexPasswordsItemSnapshots.$inferInsert
export type SelectHaexPasswordsItemSnapshots = typeof haexPasswordsItemSnapshots.$inferSelect

// Links historical attachments to snapshots (separate table because binaries
// may have been replaced on the live item while still referenced by history).
export const haexPasswordsSnapshotBinaries = sqliteTable(
  tableNames.haex.passwords_snapshot_binaries.name,
  {
    id: text(tableNames.haex.passwords_snapshot_binaries.columns.id)
      .$defaultFn(() => crypto.randomUUID())
      .primaryKey(),
    snapshotId: text(tableNames.haex.passwords_snapshot_binaries.columns.snapshotId)
      .notNull()
      .references((): AnySQLiteColumn => haexPasswordsItemSnapshots.id, { onDelete: 'cascade' }),
    binaryHash: text(tableNames.haex.passwords_snapshot_binaries.columns.binaryHash)
      .notNull()
      .references((): AnySQLiteColumn => haexPasswordsBinaries.hash, { onDelete: 'cascade' }),
    fileName: text(tableNames.haex.passwords_snapshot_binaries.columns.fileName).notNull(),
  },
)
export type InsertHaexPasswordsSnapshotBinaries = typeof haexPasswordsSnapshotBinaries.$inferInsert
export type SelectHaexPasswordsSnapshotBinaries = typeof haexPasswordsSnapshotBinaries.$inferSelect

// User-level password-generator configurations.
export const haexPasswordsGeneratorPresets = sqliteTable(
  tableNames.haex.passwords_generator_presets.name,
  {
    id: text(tableNames.haex.passwords_generator_presets.columns.id)
      .$defaultFn(() => crypto.randomUUID())
      .primaryKey(),
    name: text(tableNames.haex.passwords_generator_presets.columns.name).notNull(),
    length: integer(tableNames.haex.passwords_generator_presets.columns.length)
      .notNull()
      .default(16),
    uppercase: integer(tableNames.haex.passwords_generator_presets.columns.uppercase, {
      mode: 'boolean',
    })
      .notNull()
      .default(true),
    lowercase: integer(tableNames.haex.passwords_generator_presets.columns.lowercase, {
      mode: 'boolean',
    })
      .notNull()
      .default(true),
    numbers: integer(tableNames.haex.passwords_generator_presets.columns.numbers, {
      mode: 'boolean',
    })
      .notNull()
      .default(true),
    symbols: integer(tableNames.haex.passwords_generator_presets.columns.symbols, {
      mode: 'boolean',
    })
      .notNull()
      .default(true),
    excludeChars: text(tableNames.haex.passwords_generator_presets.columns.excludeChars).default(
      '',
    ),
    usePattern: integer(tableNames.haex.passwords_generator_presets.columns.usePattern, {
      mode: 'boolean',
    })
      .notNull()
      .default(false),
    pattern: text(tableNames.haex.passwords_generator_presets.columns.pattern).default(''),
    isDefault: integer(tableNames.haex.passwords_generator_presets.columns.isDefault, {
      mode: 'boolean',
    })
      .notNull()
      .default(false),
    createdAt: text(tableNames.haex.passwords_generator_presets.columns.createdAt).default(
      sql`(CURRENT_TIMESTAMP)`,
    ),
    updatedAt: text(tableNames.haex.passwords_generator_presets.columns.updatedAt).default(
      sql`(CURRENT_TIMESTAMP)`,
    ),
  },
)
export type InsertHaexPasswordsGeneratorPresets = typeof haexPasswordsGeneratorPresets.$inferInsert
export type SelectHaexPasswordsGeneratorPresets = typeof haexPasswordsGeneratorPresets.$inferSelect

// First-class tag entities. Name is globally unique so the permission-target
// filter ("calendar", "mail") has stable semantics for extensions.
export const haexPasswordsTags = sqliteTable(
  tableNames.haex.passwords_tags.name,
  {
    id: text(tableNames.haex.passwords_tags.columns.id)
      .$defaultFn(() => crypto.randomUUID())
      .primaryKey(),
    name: text(tableNames.haex.passwords_tags.columns.name).notNull().unique(),
    color: text(tableNames.haex.passwords_tags.columns.color),
    createdAt: text(tableNames.haex.passwords_tags.columns.createdAt).default(
      sql`(CURRENT_TIMESTAMP)`,
    ),
  },
)
export type InsertHaexPasswordsTags = typeof haexPasswordsTags.$inferInsert
export type SelectHaexPasswordsTags = typeof haexPasswordsTags.$inferSelect

// n:m junction between items and tags.
export const haexPasswordsItemTags = sqliteTable(
  tableNames.haex.passwords_item_tags.name,
  {
    id: text(tableNames.haex.passwords_item_tags.columns.id)
      .$defaultFn(() => crypto.randomUUID())
      .primaryKey(),
    itemId: text(tableNames.haex.passwords_item_tags.columns.itemId)
      .notNull()
      .references((): AnySQLiteColumn => haexPasswordsItemDetails.id, { onDelete: 'cascade' }),
    tagId: text(tableNames.haex.passwords_item_tags.columns.tagId)
      .notNull()
      .references((): AnySQLiteColumn => haexPasswordsTags.id, { onDelete: 'cascade' }),
  },
  (table) => [
    uniqueIndex('haex_passwords_item_tags_item_tag_unique').on(table.itemId, table.tagId),
  ],
)
export type InsertHaexPasswordsItemTags = typeof haexPasswordsItemTags.$inferInsert
export type SelectHaexPasswordsItemTags = typeof haexPasswordsItemTags.$inferSelect

// WebAuthn credentials. Can be linked to an item OR standalone (discoverable
// resident-key passkey). credential_id is globally unique per WebAuthn spec.
export const haexPasswordsPasskeys = sqliteTable(
  tableNames.haex.passwords_passkeys.name,
  {
    id: text(tableNames.haex.passwords_passkeys.columns.id)
      .$defaultFn(() => crypto.randomUUID())
      .primaryKey(),
    itemId: text(tableNames.haex.passwords_passkeys.columns.itemId).references(
      (): AnySQLiteColumn => haexPasswordsItemDetails.id,
      { onDelete: 'cascade' },
    ),
    credentialId: text(tableNames.haex.passwords_passkeys.columns.credentialId)
      .notNull()
      .unique(),
    relyingPartyId: text(tableNames.haex.passwords_passkeys.columns.relyingPartyId).notNull(),
    relyingPartyName: text(tableNames.haex.passwords_passkeys.columns.relyingPartyName),
    userHandle: text(tableNames.haex.passwords_passkeys.columns.userHandle).notNull(),
    userName: text(tableNames.haex.passwords_passkeys.columns.userName),
    userDisplayName: text(tableNames.haex.passwords_passkeys.columns.userDisplayName),
    // PKCS8 (private) / SPKI (public), Base64-encoded. DB is already at-rest encrypted.
    privateKey: text(tableNames.haex.passwords_passkeys.columns.privateKey).notNull(),
    publicKey: text(tableNames.haex.passwords_passkeys.columns.publicKey).notNull(),
    // COSE algorithm: -7 = ES256, -8 = EdDSA, -257 = RS256
    algorithm: integer(tableNames.haex.passwords_passkeys.columns.algorithm).notNull().default(-7),
    // Replay-protection counter, incremented on every authentication use.
    signCount: integer(tableNames.haex.passwords_passkeys.columns.signCount).notNull().default(0),
    isDiscoverable: integer(tableNames.haex.passwords_passkeys.columns.isDiscoverable, {
      mode: 'boolean',
    })
      .notNull()
      .default(true),
    icon: text(tableNames.haex.passwords_passkeys.columns.icon),
    color: text(tableNames.haex.passwords_passkeys.columns.color),
    nickname: text(tableNames.haex.passwords_passkeys.columns.nickname),
    createdAt: text(tableNames.haex.passwords_passkeys.columns.createdAt).default(
      sql`(CURRENT_TIMESTAMP)`,
    ),
    lastUsedAt: text(tableNames.haex.passwords_passkeys.columns.lastUsedAt),
  },
)
export type InsertHaexPasswordsPasskeys = typeof haexPasswordsPasskeys.$inferInsert
export type SelectHaexPasswordsPasskeys = typeof haexPasswordsPasskeys.$inferSelect
