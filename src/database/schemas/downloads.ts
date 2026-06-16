import { integer, sqliteTable, text, primaryKey } from 'drizzle-orm/sqlite-core'
import tableNames from '@/database/tableNames.json'

/**
 * Local registry of files previously downloaded from peers. Lets the file
 * browser skip a redundant re-download when the user re-clicks a remote
 * file. Keyed by (endpoint_id, remote_path) so two peers exposing files
 * with the same name can never collide.
 *
 * `local_path` holds a filesystem path on desktop and a JSON-encoded
 * Android `FileUri` (MediaStore content URI) on Android. The lookup logic
 * verifies the target still exists with the recorded size before reusing
 * it — if the user deleted the file via the OS file manager, the row is
 * dropped and the download proceeds.
 *
 * `_no_sync` because local_path is inherently per-device.
 */
export const haexPeerDownloadsNoSync = sqliteTable(
  tableNames.haex.peer_downloads_no_sync.name,
  {
    endpointId: text(tableNames.haex.peer_downloads_no_sync.columns.endpointId).notNull(),
    remotePath: text(tableNames.haex.peer_downloads_no_sync.columns.remotePath).notNull(),
    size: integer(tableNames.haex.peer_downloads_no_sync.columns.size).notNull(),
    modified: integer(tableNames.haex.peer_downloads_no_sync.columns.modified),
    localPath: text(tableNames.haex.peer_downloads_no_sync.columns.localPath).notNull(),
    downloadedAt: text(tableNames.haex.peer_downloads_no_sync.columns.downloadedAt).notNull(),
  },
  table => [primaryKey({ columns: [table.endpointId, table.remotePath] })],
)

export type InsertHaexPeerDownload = typeof haexPeerDownloadsNoSync.$inferInsert
export type SelectHaexPeerDownload = typeof haexPeerDownloadsNoSync.$inferSelect
