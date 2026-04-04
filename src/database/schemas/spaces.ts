import { sql } from 'drizzle-orm'
import {
  integer,
  sqliteTable,
  text,
  uniqueIndex,
  type AnySQLiteColumn,
} from 'drizzle-orm/sqlite-core'
import tableNames from '@/database/tableNames.json'
import { haexIdentities } from './identity'
import { haexExtensions } from './core'

// ---------------------------------------------------------------------------
// Spaces — local + remote spaces (CRDT-synced)
// ---------------------------------------------------------------------------

export const haexSpaces = sqliteTable(
  tableNames.haex.spaces.name,
  {
    id: text(tableNames.haex.spaces.columns.id).primaryKey(),
    type: text(tableNames.haex.spaces.columns.type).notNull().default('online'), // 'vault' | 'online' | 'local'
    status: text(tableNames.haex.spaces.columns.status).notNull().default('active'), // 'active' | 'pending'
    name: text(tableNames.haex.spaces.columns.name).notNull(),
    originUrl: text(tableNames.haex.spaces.columns.originUrl),
    createdAt: text(tableNames.haex.spaces.columns.createdAt).default(
      sql`(CURRENT_TIMESTAMP)`,
    ),
    modifiedAt: text(tableNames.haex.spaces.columns.modifiedAt).default(
      sql`(CURRENT_TIMESTAMP)`,
    ),
  },
)
export type InsertHaexSpaces = typeof haexSpaces.$inferInsert
export type SelectHaexSpaces = typeof haexSpaces.$inferSelect

// ---------------------------------------------------------------------------
// Space Devices — registers devices in Spaces (EndpointId → Space mapping)
// ---------------------------------------------------------------------------

export const haexSpaceDevices = sqliteTable(
  tableNames.haex.space_devices.name,
  {
    id: text(tableNames.haex.space_devices.columns.id)
      .$defaultFn(() => crypto.randomUUID())
      .primaryKey(),
    spaceId: text(tableNames.haex.space_devices.columns.spaceId)
      .notNull()
      .references(() => haexSpaces.id),
    identityId: text(tableNames.haex.space_devices.columns.identityId)
      .references(() => haexIdentities.id),
    deviceEndpointId: text(tableNames.haex.space_devices.columns.deviceEndpointId).notNull(),
    deviceName: text(tableNames.haex.space_devices.columns.deviceName).notNull(),
    avatar: text(tableNames.haex.space_devices.columns.avatar), // Base64 WebP 128x128
    avatarOptions: text(tableNames.haex.space_devices.columns.avatarOptions), // JSON DiceBear options
    relayUrl: text(tableNames.haex.space_devices.columns.relayUrl),
    leaderPriority: integer(tableNames.haex.space_devices.columns.leaderPriority).default(10),
    createdAt: text(tableNames.haex.space_devices.columns.createdAt).default(sql`(CURRENT_TIMESTAMP)`),
  },
  (table) => [
    uniqueIndex('haex_space_devices_space_device_unique').on(table.spaceId, table.deviceEndpointId),
  ],
)
export type InsertHaexSpaceDevices = typeof haexSpaceDevices.$inferInsert
export type SelectHaexSpaceDevices = typeof haexSpaceDevices.$inferSelect

// ---------------------------------------------------------------------------
// Space Members — human-readable member profiles per space (CRDT-synced)
// ---------------------------------------------------------------------------

export const haexSpaceMembers = sqliteTable(
  tableNames.haex.space_members.name,
  {
    id: text(tableNames.haex.space_members.columns.id)
      .$defaultFn(() => crypto.randomUUID())
      .primaryKey(),
    spaceId: text(tableNames.haex.space_members.columns.spaceId)
      .notNull()
      .references(() => haexSpaces.id, { onDelete: 'cascade' }),
    memberDid: text(tableNames.haex.space_members.columns.memberDid).notNull(),
    memberPublicKey: text(tableNames.haex.space_members.columns.memberPublicKey).notNull(),
    label: text(tableNames.haex.space_members.columns.label).notNull(),
    avatar: text(tableNames.haex.space_members.columns.avatar),
    avatarOptions: text(tableNames.haex.space_members.columns.avatarOptions),
    role: text(tableNames.haex.space_members.columns.role).notNull().default('read'),
    joinedAt: text(tableNames.haex.space_members.columns.joinedAt).default(
      sql`(CURRENT_TIMESTAMP)`,
    ),
  },
  (table) => [
    uniqueIndex('haex_space_members_space_did_unique').on(table.spaceId, table.memberDid),
  ],
)
export type InsertHaexSpaceMembers = typeof haexSpaceMembers.$inferInsert
export type SelectHaexSpaceMembers = typeof haexSpaceMembers.$inferSelect

// ---------------------------------------------------------------------------
// Peer Shares — folders shared in Spaces from specific devices
// ---------------------------------------------------------------------------

export const haexPeerShares = sqliteTable(
  tableNames.haex.peer_shares.name,
  {
    id: text(tableNames.haex.peer_shares.columns.id)
      .$defaultFn(() => crypto.randomUUID())
      .primaryKey(),
    spaceId: text(tableNames.haex.peer_shares.columns.spaceId)
      .notNull()
      .references(() => haexSpaces.id),
    deviceEndpointId: text(tableNames.haex.peer_shares.columns.deviceEndpointId).notNull(),
    name: text(tableNames.haex.peer_shares.columns.name).notNull(),
    localPath: text(tableNames.haex.peer_shares.columns.localPath).notNull(),
    createdAt: text(tableNames.haex.peer_shares.columns.createdAt).default(sql`(CURRENT_TIMESTAMP)`),
  },
)
export type InsertHaexPeerShares = typeof haexPeerShares.$inferInsert
export type SelectHaexPeerShares = typeof haexPeerShares.$inferSelect

// ---------------------------------------------------------------------------
// Shared Space Sync — maps rows to shared spaces for space-backend filtering
// ---------------------------------------------------------------------------

export const haexSharedSpaceSync = sqliteTable(
  tableNames.haex.shared_space_sync.name,
  {
    id: text(tableNames.haex.shared_space_sync.columns.id)
      .$defaultFn(() => crypto.randomUUID())
      .primaryKey(),
    tableName: text(tableNames.haex.shared_space_sync.columns.tableName).notNull(),
    rowPks: text(tableNames.haex.shared_space_sync.columns.rowPks, { mode: 'json' }).notNull(),
    spaceId: text(tableNames.haex.shared_space_sync.columns.spaceId)
      .notNull()
      .references(() => haexSpaces.id),
    extensionId: text(tableNames.haex.shared_space_sync.columns.extensionId)
      .references((): AnySQLiteColumn => haexExtensions.id),
    groupId: text(tableNames.haex.shared_space_sync.columns.groupId),
    type: text(tableNames.haex.shared_space_sync.columns.type),
    label: text(tableNames.haex.shared_space_sync.columns.label),
    createdAt: text(tableNames.haex.shared_space_sync.columns.createdAt).default(
      sql`(CURRENT_TIMESTAMP)`,
    ),
  },
  (table) => [
    uniqueIndex('haex_shared_space_sync_table_row_space_unique')
      .on(table.tableName, table.rowPks, table.spaceId),
  ],
)
export type InsertHaexSharedSpaceSync = typeof haexSharedSpaceSync.$inferInsert
export type SelectHaexSharedSpaceSync = typeof haexSharedSpaceSync.$inferSelect

// ---------------------------------------------------------------------------
// MLS Sync Keys — epoch-derived encryption keys for shared spaces (CRDT-synced)
// ---------------------------------------------------------------------------

export const haexMlsSyncKeys = sqliteTable(
  tableNames.haex.mls_sync_keys.name,
  {
    id: text(tableNames.haex.mls_sync_keys.columns.id)
      .$defaultFn(() => crypto.randomUUID())
      .primaryKey(),
    spaceId: text(tableNames.haex.mls_sync_keys.columns.spaceId)
      .notNull()
      .references(() => haexSpaces.id, { onDelete: 'cascade' }),
    epoch: integer(tableNames.haex.mls_sync_keys.columns.epoch).notNull(),
    keyData: text(tableNames.haex.mls_sync_keys.columns.keyData).notNull(), // Base64-encoded 32-byte key
  },
)
export type InsertHaexMlsSyncKeys = typeof haexMlsSyncKeys.$inferInsert
export type SelectHaexMlsSyncKeys = typeof haexMlsSyncKeys.$inferSelect

// ---------------------------------------------------------------------------
// Device MLS Enrollments — automatic device enrollment into MLS groups (CRDT-synced)
// ---------------------------------------------------------------------------

export const haexDeviceMlsEnrollments = sqliteTable(
  tableNames.haex.device_mls_enrollments.name,
  {
    id: text(tableNames.haex.device_mls_enrollments.columns.id)
      .$defaultFn(() => crypto.randomUUID())
      .primaryKey(),
    spaceId: text(tableNames.haex.device_mls_enrollments.columns.spaceId)
      .notNull()
      .references(() => haexSpaces.id, { onDelete: 'cascade' }),
    deviceId: text(tableNames.haex.device_mls_enrollments.columns.deviceId).notNull(),
    keyPackage: text(tableNames.haex.device_mls_enrollments.columns.keyPackage).notNull(), // Base64
    welcome: text(tableNames.haex.device_mls_enrollments.columns.welcome), // Base64, set by enrolling device
    status: text(tableNames.haex.device_mls_enrollments.columns.status).notNull().default('pending'), // 'pending' | 'enrolled'
  },
)
export type InsertHaexDeviceMlsEnrollments = typeof haexDeviceMlsEnrollments.$inferInsert
export type SelectHaexDeviceMlsEnrollments = typeof haexDeviceMlsEnrollments.$inferSelect

// ---------------------------------------------------------------------------
// UCAN Tokens — cached capability tokens for space operations
// ---------------------------------------------------------------------------

export const haexUcanTokens = sqliteTable(
  tableNames.haex.ucan_tokens.name,
  {
    id: text(tableNames.haex.ucan_tokens.columns.id).primaryKey(),
    spaceId: text(tableNames.haex.ucan_tokens.columns.spaceId)
      .notNull()
      .references(() => haexSpaces.id, { onDelete: 'cascade' }),
    token: text(tableNames.haex.ucan_tokens.columns.token).notNull(),
    capability: text(tableNames.haex.ucan_tokens.columns.capability).notNull(),
    issuerDid: text(tableNames.haex.ucan_tokens.columns.issuerDid).notNull(),
    audienceDid: text(tableNames.haex.ucan_tokens.columns.audienceDid).notNull(),
    issuedAt: integer(tableNames.haex.ucan_tokens.columns.issuedAt).notNull(),
    expiresAt: integer(tableNames.haex.ucan_tokens.columns.expiresAt).notNull(),
  },
)
export type InsertHaexUcanTokens = typeof haexUcanTokens.$inferInsert
export type SelectHaexUcanTokens = typeof haexUcanTokens.$inferSelect

// ---------------------------------------------------------------------------
// Sync Backends — backend configurations for CRDT sync
// ---------------------------------------------------------------------------

export const haexSyncBackends = sqliteTable(
  tableNames.haex.sync_backends.name,
  {
    id: text(tableNames.haex.sync_backends.columns.id)
      .$defaultFn(() => crypto.randomUUID())
      .primaryKey(),
    name: text(tableNames.haex.sync_backends.columns.name).notNull(),
    homeServerUrl: text(tableNames.haex.sync_backends.columns.homeServerUrl).notNull(),
    spaceId: text(tableNames.haex.sync_backends.columns.spaceId)
      .references(() => haexSpaces.id),
    syncKey: text(tableNames.haex.sync_backends.columns.syncKey),
    vaultKeySalt: text(tableNames.haex.sync_backends.columns.vaultKeySalt),
    identityId: text(tableNames.haex.sync_backends.columns.identityId).notNull(), // FK → haex_identities.id (UUID, for auth)
    enabled: integer(tableNames.haex.sync_backends.columns.enabled, {
      mode: 'boolean',
    })
      .default(true)
      .notNull(),
    priority: integer(tableNames.haex.sync_backends.columns.priority)
      .default(0)
      .notNull(),
    lastPushHlcTimestamp: text(tableNames.haex.sync_backends.columns.lastPushHlcTimestamp),
    lastPullServerTimestamp: text(tableNames.haex.sync_backends.columns.lastPullServerTimestamp),
    pendingVaultKeyUpdate: integer(tableNames.haex.sync_backends.columns.pendingVaultKeyUpdate, {
      mode: 'boolean',
    })
      .default(false)
      .notNull(),
    type: text(tableNames.haex.sync_backends.columns.type).notNull().default('home'),
    homeServerDid: text(tableNames.haex.sync_backends.columns.homeServerDid),
    originServerDid: text(tableNames.haex.sync_backends.columns.originServerDid),
    createdAt: text(tableNames.haex.sync_backends.columns.createdAt).default(
      sql`(CURRENT_TIMESTAMP)`,
    ),
    updatedAt: integer(tableNames.haex.sync_backends.columns.updatedAt, {
      mode: 'timestamp',
    }).$onUpdate(() => new Date()),
  },
  (table) => [
    uniqueIndex('haex_sync_backends_home_server_url_unique').on(table.homeServerUrl),
  ],
)
export type InsertHaexSyncBackends = typeof haexSyncBackends.$inferInsert
export type SelectHaexSyncBackends = typeof haexSyncBackends.$inferSelect
