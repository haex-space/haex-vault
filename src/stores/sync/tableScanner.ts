/**
 * Table Scanner - Scans CRDT tables for changes to push/pull
 * Uses Tauri commands to interact with SQLite database
 */

import { invoke } from '@tauri-apps/api/core'
import type { ColumnInfo } from '@bindings/ColumnInfo'
import type { DirtyTable } from '@bindings/DirtyTable'
import { encryptCrdtData } from '@haex-space/vault-sdk'
import tableNames from '@/database/tableNames.json'
import { hlcIsNewer } from '@/utils/hlc'
import { createLogger } from '@/stores/logging'

const CRDT_COLUMNS = tableNames.crdt.columns
const SYNC_METADATA_COLUMNS = tableNames.sync_metadata.columns

const log = createLogger('SYNC SCANNER')

export interface ColumnChange {
  tableName: string
  rowPks: string // JSON string of primary key values
  columnName: string
  hlcTimestamp: string
  encryptedValue?: string
  nonce?: string
  deviceId: string // Device that created this change
  epoch?: number // MLS epoch that encrypted this change (absent = vaultKey encrypted)
  signature?: string // Ed25519 signature over the record (present for space backends)
  signedBy?: string // Base64 SPKI public key of the signer
}

/**
 * Gets schema information for a table
 */
export async function getTableSchemaAsync(
  tableName: string,
): Promise<ColumnInfo[]> {
  return await invoke('get_table_schema', { tableName })
}

/**
 * Gets all dirty tables that need to be synced
 */
export async function getDirtyTablesAsync(): Promise<DirtyTable[]> {
  return await invoke('get_dirty_tables')
}

/**
 * Gets all CRDT-enabled tables (tables with haex_tombstone column)
 */
export async function getAllCrdtTablesAsync(): Promise<string[]> {
  return await invoke('get_all_crdt_tables')
}

/**
 * Extracts primary key values from a row
 */
export function extractPrimaryKeys(
  row: Record<string, unknown>,
  pkColumns: ColumnInfo[],
): Record<string, unknown> {
  const pks: Record<string, unknown> = {}

  for (const pkCol of pkColumns) {
    pks[pkCol.name] = row[pkCol.name]
  }

  return pks
}

/**
 * Gets the table schema split into PK columns and data columns (excluding CRDT/sync metadata).
 * Throws if the table has no primary key.
 */
async function getTableColumnsAsync(tableName: string) {
  const schema = await getTableSchemaAsync(tableName)
  const pkColumns = schema.filter((col) => col.isPk)
  const dataColumns = schema.filter(
    (col) =>
      !col.isPk &&
      col.name !== CRDT_COLUMNS.haexHlc &&
      col.name !== CRDT_COLUMNS.haexColumnHlcs &&
      col.name !== SYNC_METADATA_COLUMNS.lastPushHlcTimestamp &&
      col.name !== SYNC_METADATA_COLUMNS.lastPullServerTimestamp &&
      col.name !== SYNC_METADATA_COLUMNS.updatedAt &&
      col.name !== SYNC_METADATA_COLUMNS.createdAt,
  )

  if (pkColumns.length === 0) {
    log.error(`Table ${tableName} has no primary key`)
    throw new Error(`Table ${tableName} has no primary key`)
  }

  // The full list of columns to select (PKs + data + CRDT metadata)
  const allColumns = [
    ...pkColumns.map((c) => c.name),
    ...dataColumns.map((c) => c.name),
    CRDT_COLUMNS.haexHlc,
    CRDT_COLUMNS.haexColumnHlcs,
  ]

  return { schema, pkColumns, dataColumns, allColumns }
}

/**
 * Converts raw SQL result rows into Record objects using the known column order,
 * then processes each row into column-level changes with encryption.
 */
async function processRowsToChangesAsync(
  result: unknown[][],
  allColumns: string[],
  pkColumns: ColumnInfo[],
  dataColumns: ColumnInfo[],
  lastPushHlcTimestamp: string | null,
  tableName: string,
  vaultKey: Uint8Array,
  deviceId: string,
  epoch?: number,
): Promise<ColumnChange[]> {
  // Convert result to rows - we know the column order from allColumns
  const rows: Array<Record<string, unknown>> = []

  for (let i = 0; i < result.length; i++) {
    const row: Record<string, unknown> = {}
    const rowData = result[i]
    if (!rowData) continue

    for (let j = 0; j < allColumns.length; j++) {
      const colName = allColumns[j]
      if (colName) {
        row[colName] = rowData[j]
      }
    }
    rows.push(row)
  }

  if (rows.length > 0) {
    log.debug(`  First row PKs:`, extractPrimaryKeys(rows[0]!, pkColumns))
  }

  const changes: ColumnChange[] = []

  for (const row of rows) {
    // Parse column HLCs from JSON
    const hlcsString = row[CRDT_COLUMNS.haexColumnHlcs]

    const columnHlcs: Record<string, string> = JSON.parse(
      typeof hlcsString === 'string' ? hlcsString : '{}',
    )

    // Extract primary keys
    const pks = extractPrimaryKeys(row, pkColumns)
    const pkJson = JSON.stringify(pks)

    // Get row-level HLC to use as fallback for columns without individual HLC
    const rowHlc = row[CRDT_COLUMNS.haexHlc] as string

    // For each data column, create a change entry if it has a newer HLC
    for (const col of dataColumns) {
      const columnHlc = columnHlcs[col.name]

      // Use row-level HLC as fallback if column doesn't have individual HLC yet
      // This ensures ALL columns (including NULL values) are pushed on first sync
      const hlcToUse = columnHlc || rowHlc

      if (!hlcToUse) {
        // This should never happen as every row must have haex_hlc
        log.warn(`Column ${col.name} has no HLC and row has no haex_hlc, skipping`)
        continue
      }

      // Check if this column's HLC is newer than lastPushHlcTimestamp
      if (
        !lastPushHlcTimestamp ||
        hlcIsNewer(hlcToUse, lastPushHlcTimestamp)
      ) {
        // Encrypt the column value (wrap in object for encryptCrdtDataAsync)
        const value = row[col.name]
        const valueObject = { value }
        const { encryptedData, nonce } = await encryptCrdtData(
          valueObject as object,
          vaultKey,
        )

        changes.push({
          tableName,
          rowPks: pkJson,
          columnName: col.name,
          hlcTimestamp: hlcToUse,
          deviceId,
          encryptedValue: encryptedData,
          nonce,
          ...(epoch !== undefined && { epoch }),
        })
      }
    }
  }

  return changes
}

/**
 * Scans a table for rows that are newer than lastPushHlcTimestamp
 * Returns column-level changes for all modified columns
 */
export async function scanTableForChangesAsync(
  tableName: string,
  lastPushHlcTimestamp: string | null,
  vaultKey: Uint8Array,
  deviceId: string,
  epoch?: number,
): Promise<ColumnChange[]> {
  log.info(`Scanning table: ${tableName}`)
  log.debug(`  lastPushHlcTimestamp: ${lastPushHlcTimestamp || '(none - full scan)'}`)

  const { schema, pkColumns, dataColumns, allColumns } = await getTableColumnsAsync(tableName)
  // Note: haex_tombstone is included in dataColumns and will be synced like any other column

  log.debug(`  Schema: ${schema.length} columns, ${pkColumns.length} PKs, ${dataColumns.length} data columns`)

  const columnList = allColumns.map((c) => `"${c}"`).join(', ')

  // Note: We scan ALL rows (including tombstoned ones) that are newer than lastPushHlcTimestamp
  const whereClause = lastPushHlcTimestamp
    ? `WHERE "${CRDT_COLUMNS.haexHlc}" > ?`
    : ''
  const query = `SELECT ${columnList} FROM "${tableName}" ${whereClause}`
  const params = lastPushHlcTimestamp ? [lastPushHlcTimestamp] : []

  log.debug(`  SQL: ${query}`)
  log.debug(`  Params: ${JSON.stringify(params)}`)

  // Execute query using Tauri SQL command
  const result = await invoke<unknown[][]>('sql_select', {
    sql: query,
    params,
  })

  log.info(`  Query returned ${result.length} rows`)

  const changes = await processRowsToChangesAsync(
    result, allColumns, pkColumns, dataColumns,
    lastPushHlcTimestamp, tableName, vaultKey, deviceId, epoch,
  )

  log.info(`  Generated ${changes.length} column changes from ${result.length} rows`)

  return changes
}

/**
 * Scans a table for rows assigned to a specific shared space that are newer than lastPushHlcTimestamp.
 * Uses INNER JOIN with haex_shared_space_sync to filter only rows belonging to the space.
 * Returns empty array if no rows are assigned to this space in this table.
 */
export async function scanTableForSpaceChangesAsync(
  tableName: string,
  spaceId: string,
  lastPushHlcTimestamp: string | null,
  vaultKey: Uint8Array,
  deviceId: string,
  epoch?: number,
): Promise<ColumnChange[]> {
  log.info(`Scanning table for space: ${tableName} (spaceId: ${spaceId})`)
  log.debug(`  lastPushHlcTimestamp: ${lastPushHlcTimestamp || '(none - full scan)'}`)

  // Check if this table has any assignments for this space
  const assignmentCheck = await invoke<unknown[][]>('sql_select', {
    sql: 'SELECT 1 FROM "haex_shared_space_sync" WHERE "table_name" = ? AND "space_id" = ? LIMIT 1',
    params: [tableName, spaceId],
  })

  if (assignmentCheck.length === 0) {
    log.debug(`  No space assignments for table ${tableName}, skipping`)
    return []
  }

  const { schema, pkColumns, dataColumns, allColumns } = await getTableColumnsAsync(tableName)

  log.debug(`  Schema: ${schema.length} columns, ${pkColumns.length} PKs, ${dataColumns.length} data columns`)

  // Build column list with table alias prefix
  const columnList = allColumns.map((c) => `t."${c}"`).join(', ')

  // Build the json_object expression for PK matching
  // e.g. json_object('id', t."id") or json_object('pk1', t."pk1", 'pk2', t."pk2")
  const jsonObjectArgs = pkColumns
    .map((pk) => `'${pk.name}', t."${pk.name}"`)
    .join(', ')
  const jsonObjectExpr = `json_object(${jsonObjectArgs})`

  // Build WHERE clause for HLC filtering
  const hlcFilter = lastPushHlcTimestamp
    ? `AND t."${CRDT_COLUMNS.haexHlc}" > ?`
    : ''

  const query = `SELECT ${columnList} FROM "${tableName}" t `
    + `INNER JOIN "haex_shared_space_sync" a `
    + `ON a."table_name" = ? AND a."space_id" = ? AND a."row_pks" = ${jsonObjectExpr} `
    + `WHERE 1=1 ${hlcFilter}`

  const params: unknown[] = [tableName, spaceId]
  if (lastPushHlcTimestamp) {
    params.push(lastPushHlcTimestamp)
  }

  log.debug(`  SQL: ${query}`)
  log.debug(`  Params: ${JSON.stringify(params)}`)

  const result = await invoke<unknown[][]>('sql_select', {
    sql: query,
    params,
  })

  log.info(`  Query returned ${result.length} rows for space ${spaceId}`)

  const changes = await processRowsToChangesAsync(
    result, allColumns, pkColumns, dataColumns,
    lastPushHlcTimestamp, tableName, vaultKey, deviceId, epoch,
  )

  log.info(`  Generated ${changes.length} column changes from ${result.length} rows`)

  return changes
}

/**
 * Clears a table from the dirty tables tracker.
 * If beforeTimestamp is provided, only clears entries with last_modified <= that timestamp.
 * This prevents clearing entries that were added AFTER the sync scan started.
 */
export async function clearDirtyTableAsync(
  tableName: string,
  beforeTimestamp?: string,
): Promise<void> {
  await invoke('clear_dirty_table', { tableName, beforeTimestamp })
}

/**
 * Clears all dirty tables
 */
export async function clearAllDirtyTablesAsync(): Promise<void> {
  await invoke('clear_all_dirty_tables')
}
