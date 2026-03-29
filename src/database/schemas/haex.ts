import { sql } from 'drizzle-orm'
import {
  check,
  integer,
  sqliteTable,
  text,
  uniqueIndex,
  type AnySQLiteColumn,
} from 'drizzle-orm/sqlite-core'
import tableNames from '@/database/tableNames.json'

// Note: CRDT columns (haex_timestamp, haex_column_hlcs, haex_tombstone) are added
// automatically by the Rust CrdtTransformer when CREATE TABLE is executed.
// The WHERE haex_tombstone = 0 condition for UNIQUE indices is also added automatically.

export const haexVaultSettings = sqliteTable(
  tableNames.haex.vault_settings.name,
  {
    id: text(tableNames.haex.vault_settings.columns.id)
      .$defaultFn(() => crypto.randomUUID())
      .primaryKey(),
    key: text(tableNames.haex.vault_settings.columns.key).notNull(),
    value: text(tableNames.haex.vault_settings.columns.value),
    deviceId: text(tableNames.haex.vault_settings.columns.deviceId),
  },
  (table) => [
    uniqueIndex('haex_vault_settings_key_device_unique').on(table.key, table.deviceId),
  ],
)
export type InsertHaexVaultSettings = typeof haexVaultSettings.$inferInsert
export type SelectHaexVaultSettings = typeof haexVaultSettings.$inferSelect

// ---------------------------------------------------------------------------
// Logs — structured logging for system processes and extensions
// ---------------------------------------------------------------------------

export const haexLogs = sqliteTable(
  tableNames.haex.logs.name,
  {
    id: text(tableNames.haex.logs.columns.id)
      .$defaultFn(() => crypto.randomUUID())
      .primaryKey(),
    timestamp: text(tableNames.haex.logs.columns.timestamp).notNull(),
    level: text(tableNames.haex.logs.columns.level, {
      enum: ['debug', 'info', 'warn', 'error'],
    }).notNull(),
    source: text(tableNames.haex.logs.columns.source).notNull(),
    extensionId: text(tableNames.haex.logs.columns.extensionId)
      .references((): AnySQLiteColumn => haexExtensions.id, { onDelete: 'cascade' }),
    message: text(tableNames.haex.logs.columns.message).notNull(),
    metadata: text(tableNames.haex.logs.columns.metadata, { mode: 'json' }),
    deviceId: text(tableNames.haex.logs.columns.deviceId).notNull(),
  },
)
export type InsertHaexLogs = typeof haexLogs.$inferInsert
export type SelectHaexLogs = typeof haexLogs.$inferSelect

export const haexExtensions = sqliteTable(
  tableNames.haex.extensions.name,
  {
    id: text()
      .$defaultFn(() => crypto.randomUUID())
      .primaryKey(),
    public_key: text().notNull(),
    name: text().notNull(),
    version: text().notNull(),
    author: text(),
    description: text(),
    entry: text().default('index.html'),
    homepage: text(),
    enabled: integer({ mode: 'boolean' }).default(true),
    icon: text(),
    signature: text().notNull(),
    single_instance: integer({ mode: 'boolean' }).default(false),
    display_mode: text().default('auto'),
    // i18n overrides: { "de": { "name": "...", "description": "..." }, ... }
    i18n: text({ mode: 'json' }).$type<Record<string, { name?: string; description?: string }>>(),
    // path to dev extension project folder (if set, this is a dev extension)
    dev_path: text(),
    createdAt: text(tableNames.haex.extensions.columns.createdAt).default(
      sql`(CURRENT_TIMESTAMP)`,
    ),
    updatedAt: integer(tableNames.haex.extensions.columns.updatedAt, {
      mode: 'timestamp',
    }).$onUpdate(() => new Date()),
  },
  (table) => [
    uniqueIndex('haex_extensions_public_key_name_unique').on(table.public_key, table.name),
  ],
)
export type InsertHaexExtensions = typeof haexExtensions.$inferInsert
export type SelectHaexExtensions = typeof haexExtensions.$inferSelect

export const haexExtensionPermissions = sqliteTable(
  tableNames.haex.extension_permissions.name,
  {
    id: text()
      .$defaultFn(() => crypto.randomUUID())
      .primaryKey(),
    extensionId: text(tableNames.haex.extension_permissions.columns.extensionId)
      .notNull()
      .references((): AnySQLiteColumn => haexExtensions.id, {
        onDelete: 'cascade',
      }),
    resourceType: text('resource_type', {
      enum: ['fs', 'web', 'db', 'shell'],
    }),
    action: text({ enum: ['read', 'write'] }),
    target: text(),
    constraints: text({ mode: 'json' }),
    status: text({ enum: ['ask', 'granted', 'denied'] })
      .notNull()
      .default('denied'),
    createdAt: text('created_at').default(sql`(CURRENT_TIMESTAMP)`),
    updateAt: integer('updated_at', { mode: 'timestamp' }).$onUpdate(
      () => new Date(),
    ),
  },
  (table) => [
    uniqueIndex('haex_extension_permissions_extension_id_resource_type_action_target_unique')
      .on(table.extensionId, table.resourceType, table.action, table.target),
  ],
)
export type InserthaexExtensionPermissions =
  typeof haexExtensionPermissions.$inferInsert
export type SelecthaexExtensionPermissions =
  typeof haexExtensionPermissions.$inferSelect

export const haexNotifications = sqliteTable(
  tableNames.haex.notifications.name,
  {
    id: text()
      .$defaultFn(() => crypto.randomUUID())
      .primaryKey(),
    alt: text(),
    date: text(),
    icon: text(),
    image: text(),
    read: integer({ mode: 'boolean' }),
    source: text(),
    text: text(),
    title: text(),
    type: text({
      enum: ['error', 'success', 'warning', 'info', 'log'],
    }).notNull(),
  },
)
export type InsertHaexNotifications = typeof haexNotifications.$inferInsert
export type SelectHaexNotifications = typeof haexNotifications.$inferSelect

export const haexWorkspaces = sqliteTable(
  tableNames.haex.workspaces.name,
  {
    id: text(tableNames.haex.workspaces.columns.id)
      .$defaultFn(() => crypto.randomUUID())
      .primaryKey(),
    deviceId: text(tableNames.haex.workspaces.columns.deviceId).notNull(),
    name: text(tableNames.haex.workspaces.columns.name).notNull(),
    position: integer(tableNames.haex.workspaces.columns.position)
      .notNull()
      .default(0),
    background: text(),
  },
  (table) => [
    uniqueIndex('haex_workspaces_device_position_unique').on(table.deviceId, table.position),
  ],
)
export type InsertHaexWorkspaces = typeof haexWorkspaces.$inferInsert
export type SelectHaexWorkspaces = typeof haexWorkspaces.$inferSelect

export const haexDesktopItems = sqliteTable(
  tableNames.haex.desktop_items.name,
  {
    id: text(tableNames.haex.desktop_items.columns.id)
      .$defaultFn(() => crypto.randomUUID())
      .primaryKey(),
    workspaceId: text(tableNames.haex.desktop_items.columns.workspaceId)
      .notNull()
      .references(() => haexWorkspaces.id, { onDelete: 'cascade' }),
    itemType: text(tableNames.haex.desktop_items.columns.itemType, {
      enum: ['system', 'extension', 'file', 'folder'],
    }).notNull(),
    // Für Extensions (wenn itemType = 'extension')
    extensionId: text(
      tableNames.haex.desktop_items.columns.extensionId,
    ).references((): AnySQLiteColumn => haexExtensions.id, {
      onDelete: 'cascade',
    }),
    // Für System Windows (wenn itemType = 'system')
    systemWindowId: text(tableNames.haex.desktop_items.columns.systemWindowId),
    positionX: integer(tableNames.haex.desktop_items.columns.positionX)
      .notNull()
      .default(0),
    positionY: integer(tableNames.haex.desktop_items.columns.positionY)
      .notNull()
      .default(0),
  },
  (table) => [
    check(
      'item_reference',
      sql`(${table.itemType} = 'extension' AND ${table.extensionId} IS NOT NULL AND ${table.systemWindowId} IS NULL) OR (${table.itemType} = 'system' AND ${table.systemWindowId} IS NOT NULL AND ${table.extensionId} IS NULL) OR (${table.itemType} = 'file' AND ${table.systemWindowId} IS NOT NULL AND ${table.extensionId} IS NULL) OR (${table.itemType} = 'folder' AND ${table.systemWindowId} IS NOT NULL AND ${table.extensionId} IS NULL)`,
    ),
  ],
)
export type InsertHaexDesktopItems = typeof haexDesktopItems.$inferInsert
export type SelectHaexDesktopItems = typeof haexDesktopItems.$inferSelect

export const haexSyncBackends = sqliteTable(
  tableNames.haex.sync_backends.name,
  {
    id: text(tableNames.haex.sync_backends.columns.id)
      .$defaultFn(() => crypto.randomUUID())
      .primaryKey(),
    name: text(tableNames.haex.sync_backends.columns.name).notNull(),
    serverUrl: text(tableNames.haex.sync_backends.columns.serverUrl).notNull(),
    spaceId: text(tableNames.haex.sync_backends.columns.spaceId)
      .references(() => haexSpaces.id),
    syncKey: text(tableNames.haex.sync_backends.columns.syncKey),
    vaultKeySalt: text(tableNames.haex.sync_backends.columns.vaultKeySalt),
    identityId: text(tableNames.haex.sync_backends.columns.identityId), // FK → haex_identities.publicKey (for auth)
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
    createdAt: text(tableNames.haex.sync_backends.columns.createdAt).default(
      sql`(CURRENT_TIMESTAMP)`,
    ),
    updatedAt: integer(tableNames.haex.sync_backends.columns.updatedAt, {
      mode: 'timestamp',
    }).$onUpdate(() => new Date()),
  },
  (table) => [
    uniqueIndex('haex_sync_backends_server_url_unique').on(table.serverUrl),
  ],
)
export type InsertHaexSyncBackends = typeof haexSyncBackends.$inferInsert
export type SelectHaexSyncBackends = typeof haexSyncBackends.$inferSelect

export const haexExtensionMigrations = sqliteTable(
  tableNames.haex.extension_migrations.name,
  {
    id: text(tableNames.haex.extension_migrations.columns.id)
      .$defaultFn(() => crypto.randomUUID())
      .primaryKey(),
    extensionId: text(tableNames.haex.extension_migrations.columns.extensionId)
      .notNull()
      .references((): AnySQLiteColumn => haexExtensions.id, {
        onDelete: 'cascade',
      }),
    extensionVersion: text(tableNames.haex.extension_migrations.columns.extensionVersion).notNull(),
    migrationName: text(tableNames.haex.extension_migrations.columns.migrationName).notNull(),
    sqlStatement: text(tableNames.haex.extension_migrations.columns.sqlStatement).notNull(),
  },
  (table) => [
    uniqueIndex('haex_extension_migrations_extension_id_migration_name_unique')
      .on(table.extensionId, table.migrationName),
  ],
)
export type InsertHaexExtensionMigrations = typeof haexExtensionMigrations.$inferInsert
export type SelectHaexExtensionMigrations = typeof haexExtensionMigrations.$inferSelect

// External authorized clients (browser extensions, CLI tools, servers, etc.)
export const haexExternalAuthorizedClients = sqliteTable(
  tableNames.haex.external_authorized_clients.name,
  {
    id: text(tableNames.haex.external_authorized_clients.columns.id)
      .$defaultFn(() => crypto.randomUUID())
      .primaryKey(),
    clientId: text(tableNames.haex.external_authorized_clients.columns.clientId).notNull(),
    clientName: text(tableNames.haex.external_authorized_clients.columns.clientName).notNull(),
    publicKey: text(tableNames.haex.external_authorized_clients.columns.publicKey).notNull(),
    extensionId: text(tableNames.haex.external_authorized_clients.columns.extensionId)
      .notNull()
      .references((): AnySQLiteColumn => haexExtensions.id, {
        onDelete: 'cascade',
      }),
    authorizedAt: text(tableNames.haex.external_authorized_clients.columns.authorizedAt).default(
      sql`(CURRENT_TIMESTAMP)`,
    ),
    lastSeen: text(tableNames.haex.external_authorized_clients.columns.lastSeen),
  },
  (table) => [
    uniqueIndex('haex_external_authorized_clients_client_extension_unique')
      .on(table.clientId, table.extensionId),
  ],
)
export type InsertHaexExternalAuthorizedClients = typeof haexExternalAuthorizedClients.$inferInsert
export type SelectHaexExternalAuthorizedClients = typeof haexExternalAuthorizedClients.$inferSelect

// External blocked clients (permanently denied)
export const haexExternalBlockedClients = sqliteTable(
  tableNames.haex.external_blocked_clients.name,
  {
    id: text(tableNames.haex.external_blocked_clients.columns.id)
      .$defaultFn(() => crypto.randomUUID())
      .primaryKey(),
    clientId: text(tableNames.haex.external_blocked_clients.columns.clientId).notNull(),
    clientName: text(tableNames.haex.external_blocked_clients.columns.clientName).notNull(),
    publicKey: text(tableNames.haex.external_blocked_clients.columns.publicKey).notNull(),
    blockedAt: text(tableNames.haex.external_blocked_clients.columns.blockedAt).default(
      sql`(CURRENT_TIMESTAMP)`,
    ),
  },
  (table) => [
    uniqueIndex('haex_external_blocked_clients_client_id_unique').on(table.clientId),
  ],
)
export type InsertHaexExternalBlockedClients = typeof haexExternalBlockedClients.$inferInsert
export type SelectHaexExternalBlockedClients = typeof haexExternalBlockedClients.$inferSelect

// Extension resource limits (query timeouts, result size limits, etc.)
export const haexExtensionLimits = sqliteTable(
  tableNames.haex.extension_limits.name,
  {
    id: text(tableNames.haex.extension_limits.columns.id)
      .$defaultFn(() => crypto.randomUUID())
      .primaryKey(),
    extensionId: text(tableNames.haex.extension_limits.columns.extensionId)
      .notNull()
      .references((): AnySQLiteColumn => haexExtensions.id, {
        onDelete: 'cascade',
      }),
    // Query timeout in milliseconds (default: 30000 = 30 seconds)
    queryTimeoutMs: integer(tableNames.haex.extension_limits.columns.queryTimeoutMs)
      .notNull()
      .default(30000),
    // Maximum number of rows returned per query (default: 10000)
    maxResultRows: integer(tableNames.haex.extension_limits.columns.maxResultRows)
      .notNull()
      .default(10000),
    // Maximum concurrent queries per extension (default: 5)
    maxConcurrentQueries: integer(tableNames.haex.extension_limits.columns.maxConcurrentQueries)
      .notNull()
      .default(5),
    // Maximum query SQL size in bytes (default: 1MB)
    maxQuerySizeBytes: integer(tableNames.haex.extension_limits.columns.maxQuerySizeBytes)
      .notNull()
      .default(1048576),
    createdAt: text(tableNames.haex.extension_limits.columns.createdAt).default(sql`(CURRENT_TIMESTAMP)`),
    updatedAt: integer(tableNames.haex.extension_limits.columns.updatedAt, { mode: 'timestamp' })
      .$onUpdate(() => new Date()),
  },
  (table) => [
    uniqueIndex('haex_extension_limits_extension_id_unique').on(table.extensionId),
  ],
)
export type InsertHaexExtensionLimits = typeof haexExtensionLimits.$inferInsert
export type SelectHaexExtensionLimits = typeof haexExtensionLimits.$inferSelect

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
    privateKey: text(tableNames.haex.identities.columns.privateKey).notNull(), // Base64 PKCS8 Ed25519 signing key
    agreementPublicKey: text(tableNames.haex.identities.columns.agreementPublicKey).notNull(), // Base64 SPKI X25519
    agreementPrivateKey: text(tableNames.haex.identities.columns.agreementPrivateKey).notNull(), // Base64 PKCS8 X25519
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

// ---------------------------------------------------------------------------
// Spaces — local + remote spaces (CRDT-synced)
// ---------------------------------------------------------------------------

export const haexSpaces = sqliteTable(
  tableNames.haex.spaces.name,
  {
    id: text(tableNames.haex.spaces.columns.id).primaryKey(),
    name: text(tableNames.haex.spaces.columns.name).notNull(),
    serverUrl: text(tableNames.haex.spaces.columns.serverUrl),
    role: text(tableNames.haex.spaces.columns.role).notNull(),
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
      .references(() => haexIdentities.publicKey),
    deviceEndpointId: text(tableNames.haex.space_devices.columns.deviceEndpointId).notNull(),
    deviceName: text(tableNames.haex.space_devices.columns.deviceName).notNull(),
    avatar: text(tableNames.haex.space_devices.columns.avatar), // Base64 WebP 128x128
    relayUrl: text(tableNames.haex.space_devices.columns.relayUrl),
    createdAt: text(tableNames.haex.space_devices.columns.createdAt).default(sql`(CURRENT_TIMESTAMP)`),
  },
  (table) => [
    uniqueIndex('haex_space_devices_space_device_unique').on(table.spaceId, table.deviceEndpointId),
  ],
)

export type InsertHaexSpaceDevices = typeof haexSpaceDevices.$inferInsert
export type SelectHaexSpaceDevices = typeof haexSpaceDevices.$inferSelect

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
// Space Keys — persisted space decryption keys (CRDT-synced)
// Multiple keys per (spaceId, generation) are allowed for offline conflict resolution
// ---------------------------------------------------------------------------

export const haexSpaceKeys = sqliteTable(
  tableNames.haex.space_keys.name,
  {
    id: text(tableNames.haex.space_keys.columns.id)
      .$defaultFn(() => crypto.randomUUID())
      .primaryKey(),
    spaceId: text(tableNames.haex.space_keys.columns.spaceId)
      .notNull()
      .references(() => haexSpaces.id),
    generation: integer(tableNames.haex.space_keys.columns.generation).notNull(),
    key: text(tableNames.haex.space_keys.columns.key).notNull(),
  },
)

export type InsertHaexSpaceKeys = typeof haexSpaceKeys.$inferInsert
export type SelectHaexSpaceKeys = typeof haexSpaceKeys.$inferSelect

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
// Pending Invites — incoming space invitations awaiting user response
// ---------------------------------------------------------------------------

export const haexPendingInvites = sqliteTable(
  tableNames.haex.pending_invites.name,
  {
    id: text(tableNames.haex.pending_invites.columns.id).primaryKey(),
    spaceId: text(tableNames.haex.pending_invites.columns.spaceId)
      .notNull()
      .references(() => haexSpaces.id, { onDelete: 'cascade' }),
    inviterDid: text(tableNames.haex.pending_invites.columns.inviterDid).notNull(),
    inviterLabel: text(tableNames.haex.pending_invites.columns.inviterLabel),
    spaceName: text(tableNames.haex.pending_invites.columns.spaceName),
    status: text(tableNames.haex.pending_invites.columns.status).notNull().default('pending'),
    includeHistory: integer(tableNames.haex.pending_invites.columns.includeHistory, { mode: 'boolean' }).default(false),
    createdAt: text(tableNames.haex.pending_invites.columns.createdAt).notNull(),
    respondedAt: text(tableNames.haex.pending_invites.columns.respondedAt),
  },
)
export type InsertHaexPendingInvites = typeof haexPendingInvites.$inferInsert
export type SelectHaexPendingInvites = typeof haexPendingInvites.$inferSelect

// ---------------------------------------------------------------------------
// Blocked DIDs — permanently blocked identities
// ---------------------------------------------------------------------------

export const haexBlockedDids = sqliteTable(
  tableNames.haex.blocked_dids.name,
  {
    id: text(tableNames.haex.blocked_dids.columns.id).primaryKey(),
    did: text(tableNames.haex.blocked_dids.columns.did).notNull().unique(),
    label: text(tableNames.haex.blocked_dids.columns.label),
    blockedAt: text(tableNames.haex.blocked_dids.columns.blockedAt).notNull(),
  },
)
export type InsertHaexBlockedDids = typeof haexBlockedDids.$inferInsert
export type SelectHaexBlockedDids = typeof haexBlockedDids.$inferSelect

// ---------------------------------------------------------------------------
// Invite Policy — controls who can send space invitations
// ---------------------------------------------------------------------------

export const haexInvitePolicy = sqliteTable(
  tableNames.haex.invite_policy.name,
  {
    id: text(tableNames.haex.invite_policy.columns.id).primaryKey(),
    policy: text(tableNames.haex.invite_policy.columns.policy).notNull().default('all'),
    updatedAt: text(tableNames.haex.invite_policy.columns.updatedAt).notNull(),
  },
)
export type InsertHaexInvitePolicy = typeof haexInvitePolicy.$inferInsert
export type SelectHaexInvitePolicy = typeof haexInvitePolicy.$inferSelect

