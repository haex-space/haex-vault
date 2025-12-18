import { sql } from 'drizzle-orm'
import {
  index,
  integer,
  sqliteTable,
  text,
  uniqueIndex,
  type AnySQLiteColumn,
} from 'drizzle-orm/sqlite-core'
import tableNames from '@/database/tableNames.json'
import { withCrdtAndTimestamps, withCrdtColumns } from './haex'

// ============================================================================
// File Spaces (like folders/buckets for organizing files)
// ============================================================================

export const haexFileSpaces = sqliteTable(
  tableNames.haex.file_spaces.name,
  withCrdtAndTimestamps(
    {
      id: text(tableNames.haex.file_spaces.columns.id)
        .$defaultFn(() => crypto.randomUUID())
        .primaryKey(),
      name: text(tableNames.haex.file_spaces.columns.name).notNull(),
      isPersonal: integer(tableNames.haex.file_spaces.columns.isPersonal, {
        mode: 'boolean',
      })
        .notNull()
        .default(true),
      // Wrapped with vault_key, contains the space encryption key
      wrappedKey: text(tableNames.haex.file_spaces.columns.wrappedKey).notNull(),
      fileCount: integer(tableNames.haex.file_spaces.columns.fileCount)
        .notNull()
        .default(0),
      totalSize: integer(tableNames.haex.file_spaces.columns.totalSize)
        .notNull()
        .default(0),
    },
    tableNames.haex.file_spaces.columns.createdAt,
    tableNames.haex.file_spaces.columns.updatedAt,
  ),
  (table) => [
    uniqueIndex('haex_file_spaces_name_unique')
      .on(table.name)
      .where(sql`${table.haexTombstone} = 0`),
  ],
)
export type InsertHaexFileSpaces = typeof haexFileSpaces.$inferInsert
export type SelectHaexFileSpaces = typeof haexFileSpaces.$inferSelect

// ============================================================================
// Storage Backends (S3, R2, MinIO, etc.)
// ============================================================================

export const haexFileBackends = sqliteTable(
  tableNames.haex.file_backends.name,
  withCrdtAndTimestamps(
    {
      id: text(tableNames.haex.file_backends.columns.id)
        .$defaultFn(() => crypto.randomUUID())
        .primaryKey(),
      type: text(tableNames.haex.file_backends.columns.type, {
        enum: ['s3', 'r2', 'minio', 'gdrive', 'dropbox'],
      }).notNull(),
      name: text(tableNames.haex.file_backends.columns.name).notNull(),
      // Encrypted JSON config (with vault_key) containing credentials
      encryptedConfig: text(tableNames.haex.file_backends.columns.encryptedConfig).notNull(),
      enabled: integer(tableNames.haex.file_backends.columns.enabled, {
        mode: 'boolean',
      })
        .notNull()
        .default(true),
    },
    tableNames.haex.file_backends.columns.createdAt,
    tableNames.haex.file_backends.columns.updatedAt,
  ),
  (table) => [
    uniqueIndex('haex_file_backends_name_unique')
      .on(table.name)
      .where(sql`${table.haexTombstone} = 0`),
  ],
)
export type InsertHaexFileBackends = typeof haexFileBackends.$inferInsert
export type SelectHaexFileBackends = typeof haexFileBackends.$inferSelect

// ============================================================================
// Files (encrypted metadata index)
// ============================================================================

export const haexFiles = sqliteTable(
  tableNames.haex.files.name,
  withCrdtAndTimestamps(
    {
      id: text(tableNames.haex.files.columns.id)
        .$defaultFn(() => crypto.randomUUID())
        .primaryKey(),
      spaceId: text(tableNames.haex.files.columns.spaceId)
        .notNull()
        .references((): AnySQLiteColumn => haexFileSpaces.id, {
          onDelete: 'cascade',
        }),
      parentId: text(tableNames.haex.files.columns.parentId).references(
        (): AnySQLiteColumn => haexFiles.id,
        { onDelete: 'cascade' },
      ),
      // Encrypted with space_key
      encryptedName: text(tableNames.haex.files.columns.encryptedName).notNull(),
      encryptedPath: text(tableNames.haex.files.columns.encryptedPath).notNull(),
      encryptedMimeType: text(tableNames.haex.files.columns.encryptedMimeType),
      isDirectory: integer(tableNames.haex.files.columns.isDirectory, {
        mode: 'boolean',
      })
        .notNull()
        .default(false),
      size: integer(tableNames.haex.files.columns.size).notNull().default(0),
      // Hash of original plaintext content (for dedup)
      contentHash: text(tableNames.haex.files.columns.contentHash),
      // Per-file encryption key, wrapped with space_key
      wrappedKey: text(tableNames.haex.files.columns.wrappedKey),
      chunkCount: integer(tableNames.haex.files.columns.chunkCount)
        .notNull()
        .default(0),
      syncState: text(tableNames.haex.files.columns.syncState, {
        enum: ['synced', 'syncing', 'local_only', 'remote_only', 'conflict', 'error'],
      })
        .notNull()
        .default('local_only'),
    },
    tableNames.haex.files.columns.createdAt,
    tableNames.haex.files.columns.updatedAt,
  ),
  (table) => [
    index('haex_files_space_id_idx').on(table.spaceId),
    index('haex_files_parent_id_idx').on(table.parentId),
    index('haex_files_sync_state_idx').on(table.syncState),
  ],
)
export type InsertHaexFiles = typeof haexFiles.$inferInsert
export type SelectHaexFiles = typeof haexFiles.$inferSelect

// ============================================================================
// File Chunks (for large files)
// ============================================================================

export const haexFileChunks = sqliteTable(
  tableNames.haex.file_chunks.name,
  withCrdtColumns({
    id: text(tableNames.haex.file_chunks.columns.id)
      .$defaultFn(() => crypto.randomUUID())
      .primaryKey(),
    fileId: text(tableNames.haex.file_chunks.columns.fileId)
      .notNull()
      .references((): AnySQLiteColumn => haexFiles.id, {
        onDelete: 'cascade',
      }),
    chunkIndex: integer(tableNames.haex.file_chunks.columns.chunkIndex).notNull(),
    // UUID for remote storage (blob name)
    remoteId: text(tableNames.haex.file_chunks.columns.remoteId),
    size: integer(tableNames.haex.file_chunks.columns.size).notNull(),
    // Hash of encrypted chunk (for verification)
    encryptedHash: text(tableNames.haex.file_chunks.columns.encryptedHash).notNull(),
    uploaded: integer(tableNames.haex.file_chunks.columns.uploaded, {
      mode: 'boolean',
    })
      .notNull()
      .default(false),
    createdAt: text(tableNames.haex.file_chunks.columns.createdAt).default(
      sql`(CURRENT_TIMESTAMP)`,
    ),
  }),
  (table) => [
    uniqueIndex('haex_file_chunks_file_chunk_unique')
      .on(table.fileId, table.chunkIndex)
      .where(sql`${table.haexTombstone} = 0`),
    index('haex_file_chunks_file_id_idx').on(table.fileId),
  ],
)
export type InsertHaexFileChunks = typeof haexFileChunks.$inferInsert
export type SelectHaexFileChunks = typeof haexFileChunks.$inferSelect

// ============================================================================
// File to Backend Mapping (which backends store which files)
// ============================================================================

export const haexFileBackendMapping = sqliteTable(
  tableNames.haex.file_backend_mapping.name,
  withCrdtColumns({
    id: text(tableNames.haex.file_backend_mapping.columns.id)
      .$defaultFn(() => crypto.randomUUID())
      .primaryKey(),
    fileId: text(tableNames.haex.file_backend_mapping.columns.fileId)
      .notNull()
      .references((): AnySQLiteColumn => haexFiles.id, {
        onDelete: 'cascade',
      }),
    backendId: text(tableNames.haex.file_backend_mapping.columns.backendId)
      .notNull()
      .references((): AnySQLiteColumn => haexFileBackends.id, {
        onDelete: 'cascade',
      }),
    // UUID used as remote blob name/key
    remoteId: text(tableNames.haex.file_backend_mapping.columns.remoteId).notNull(),
    uploadedAt: text(tableNames.haex.file_backend_mapping.columns.uploadedAt),
    verifiedAt: text(tableNames.haex.file_backend_mapping.columns.verifiedAt),
  }),
  (table) => [
    uniqueIndex('haex_file_backend_mapping_file_backend_unique')
      .on(table.fileId, table.backendId)
      .where(sql`${table.haexTombstone} = 0`),
    index('haex_file_backend_mapping_file_id_idx').on(table.fileId),
    index('haex_file_backend_mapping_backend_id_idx').on(table.backendId),
  ],
)
export type InsertHaexFileBackendMapping = typeof haexFileBackendMapping.$inferInsert
export type SelectHaexFileBackendMapping = typeof haexFileBackendMapping.$inferSelect

// ============================================================================
// Sync Rules (local folder → space/backends mapping)
// ============================================================================

export const haexFileSyncRules = sqliteTable(
  tableNames.haex.file_sync_rules.name,
  withCrdtAndTimestamps(
    {
      id: text(tableNames.haex.file_sync_rules.columns.id)
        .$defaultFn(() => crypto.randomUUID())
        .primaryKey(),
      // Device ID - sync rules are device-specific because local paths differ per device
      deviceId: text(tableNames.haex.file_sync_rules.columns.deviceId).notNull(),
      spaceId: text(tableNames.haex.file_sync_rules.columns.spaceId)
        .notNull()
        .references((): AnySQLiteColumn => haexFileSpaces.id, {
          onDelete: 'cascade',
        }),
      localPath: text(tableNames.haex.file_sync_rules.columns.localPath).notNull(),
      direction: text(tableNames.haex.file_sync_rules.columns.direction, {
        enum: ['up', 'down', 'both'],
      })
        .notNull()
        .default('both'),
      enabled: integer(tableNames.haex.file_sync_rules.columns.enabled, {
        mode: 'boolean',
      })
        .notNull()
        .default(true),
      lastSyncAt: text(tableNames.haex.file_sync_rules.columns.lastSyncAt),
    },
    tableNames.haex.file_sync_rules.columns.createdAt,
    tableNames.haex.file_sync_rules.columns.updatedAt,
  ),
  (table) => [
    // Unique constraint on device_id + local_path (same path can be synced on different devices)
    uniqueIndex('haex_file_sync_rules_device_path_unique')
      .on(table.deviceId, table.localPath)
      .where(sql`${table.haexTombstone} = 0`),
    index('haex_file_sync_rules_space_id_idx').on(table.spaceId),
    index('haex_file_sync_rules_device_id_idx').on(table.deviceId),
  ],
)
export type InsertHaexFileSyncRules = typeof haexFileSyncRules.$inferInsert
export type SelectHaexFileSyncRules = typeof haexFileSyncRules.$inferSelect

// ============================================================================
// Sync Rule to Backend Mapping
// ============================================================================

export const haexFileSyncRuleBackends = sqliteTable(
  tableNames.haex.file_sync_rule_backends.name,
  withCrdtColumns({
    id: text(tableNames.haex.file_sync_rule_backends.columns.id)
      .$defaultFn(() => crypto.randomUUID())
      .primaryKey(),
    ruleId: text(tableNames.haex.file_sync_rule_backends.columns.ruleId)
      .notNull()
      .references((): AnySQLiteColumn => haexFileSyncRules.id, {
        onDelete: 'cascade',
      }),
    backendId: text(tableNames.haex.file_sync_rule_backends.columns.backendId)
      .notNull()
      .references((): AnySQLiteColumn => haexFileBackends.id, {
        onDelete: 'cascade',
      }),
  }),
  (table) => [
    uniqueIndex('haex_file_sync_rule_backends_rule_backend_unique')
      .on(table.ruleId, table.backendId)
      .where(sql`${table.haexTombstone} = 0`),
  ],
)
export type InsertHaexFileSyncRuleBackends = typeof haexFileSyncRuleBackends.$inferInsert
export type SelectHaexFileSyncRuleBackends = typeof haexFileSyncRuleBackends.$inferSelect

// ============================================================================
// Local Sync State (tracks what has been synced locally)
// ============================================================================

export const haexFileLocalSyncState = sqliteTable(
  tableNames.haex.file_local_sync_state.name,
  withCrdtColumns({
    id: text(tableNames.haex.file_local_sync_state.columns.id)
      .$defaultFn(() => crypto.randomUUID())
      .primaryKey(),
    // Device ID - same file can have different local states on different devices
    deviceId: text(tableNames.haex.file_local_sync_state.columns.deviceId).notNull(),
    fileId: text(tableNames.haex.file_local_sync_state.columns.fileId)
      .notNull()
      .references((): AnySQLiteColumn => haexFiles.id, {
        onDelete: 'cascade',
      }),
    localPath: text(tableNames.haex.file_local_sync_state.columns.localPath).notNull(),
    localHash: text(tableNames.haex.file_local_sync_state.columns.localHash).notNull(),
    localMtime: text(tableNames.haex.file_local_sync_state.columns.localMtime).notNull(),
    localSize: integer(tableNames.haex.file_local_sync_state.columns.localSize).notNull(),
    syncedAt: text(tableNames.haex.file_local_sync_state.columns.syncedAt),
  }),
  (table) => [
    // Unique constraint on device_id + file_id (same file can exist on multiple devices)
    uniqueIndex('haex_file_local_sync_state_device_file_unique')
      .on(table.deviceId, table.fileId)
      .where(sql`${table.haexTombstone} = 0`),
    // Unique constraint on device_id + local_path (same path can be used on different devices)
    uniqueIndex('haex_file_local_sync_state_device_path_unique')
      .on(table.deviceId, table.localPath)
      .where(sql`${table.haexTombstone} = 0`),
    index('haex_file_local_sync_state_device_id_idx').on(table.deviceId),
  ],
)
export type InsertHaexFileLocalSyncState = typeof haexFileLocalSyncState.$inferInsert
export type SelectHaexFileLocalSyncState = typeof haexFileLocalSyncState.$inferSelect

// ============================================================================
// Sync Errors Log
// ============================================================================

export const haexFileSyncErrors = sqliteTable(
  tableNames.haex.file_sync_errors.name,
  withCrdtColumns({
    id: text(tableNames.haex.file_sync_errors.columns.id)
      .$defaultFn(() => crypto.randomUUID())
      .primaryKey(),
    fileId: text(tableNames.haex.file_sync_errors.columns.fileId).references(
      (): AnySQLiteColumn => haexFiles.id,
      { onDelete: 'set null' },
    ),
    backendId: text(tableNames.haex.file_sync_errors.columns.backendId).references(
      (): AnySQLiteColumn => haexFileBackends.id,
      { onDelete: 'set null' },
    ),
    errorType: text(tableNames.haex.file_sync_errors.columns.errorType, {
      enum: ['upload', 'download', 'delete', 'connection', 'encryption', 'other'],
    }).notNull(),
    errorMessage: text(tableNames.haex.file_sync_errors.columns.errorMessage).notNull(),
    retryCount: integer(tableNames.haex.file_sync_errors.columns.retryCount)
      .notNull()
      .default(0),
    lastRetryAt: text(tableNames.haex.file_sync_errors.columns.lastRetryAt),
    resolved: integer(tableNames.haex.file_sync_errors.columns.resolved, {
      mode: 'boolean',
    })
      .notNull()
      .default(false),
    createdAt: text(tableNames.haex.file_sync_errors.columns.createdAt).default(
      sql`(CURRENT_TIMESTAMP)`,
    ),
  }),
  (table) => [index('haex_file_sync_errors_resolved_idx').on(table.resolved)],
)
export type InsertHaexFileSyncErrors = typeof haexFileSyncErrors.$inferInsert
export type SelectHaexFileSyncErrors = typeof haexFileSyncErrors.$inferSelect
