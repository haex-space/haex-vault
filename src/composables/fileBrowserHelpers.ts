import { save as showSaveDialog } from '@tauri-apps/plugin-dialog'
import { writeFile } from '@tauri-apps/plugin-fs'
import type { FileEntry as BaseFileEntry } from '~/../src-tauri/bindings/FileEntry'

// Extended FileEntry with optional path (used on Android for Content URIs)
// and an optional spaceId for entries that came from the multi-space root
// listing (`remoteListAllSharesAsync`). The spaceId carries provenance
// through subsequent calls so a click on `TestShare` in space A always
// resolves the UCAN of space A, even when space B also has a `TestShare`.
export type FileEntry = BaseFileEntry & { path?: string; spaceId?: string }

export interface RemotePeer {
  endpointId: string
  name: string
  source: 'space' | 'contact' | 's3'
  detail: string
  localPath?: string
  /**
   * When set, the peer represents a remote S3 backend rather than a P2P
   * endpoint. `endpointId` is then a synthetic id (e.g. `s3:<backendId>`)
   * used only for UI bookkeeping — backend calls use `s3BackendId`.
   */
  s3BackendId?: string
}

export type ViewMode = 'list' | 'grid'

// Search result entry with path context for deep (recursive) search
export type SearchableFile = FileEntry & {
  displayPath: string  // Relative directory for UI display, e.g. "photos/vacation"
  searchPath: string   // Full path from share root, e.g. "/photos/vacation/beach.jpg"
}

// Global search result across all local shares
export type GlobalSearchFile = SearchableFile & {
  shareId: string
  shareName: string
  shareLocalPath: string
}

export const CHUNK_SIZE = 50

/**
 * Feature flag — whether the iroh P2P transport currently exposes mutation
 * verbs (delete, rename, cross-peer copy). When this flips to `true`, the
 * file browser's per-row context menu starts surfacing those actions for
 * P2P peers, still gated by a write UCAN (`canWrite`). Flip on
 * feat/p2p-mutation-ops once the backend lands.
 */
export const P2P_MUTATIONS_SUPPORTED = false

const FILE_ICONS: Record<string, string> = {
  jpg: 'i-lucide-image', jpeg: 'i-lucide-image', png: 'i-lucide-image',
  gif: 'i-lucide-image', webp: 'i-lucide-image', svg: 'i-lucide-image',
  mp4: 'i-lucide-video', mov: 'i-lucide-video', avi: 'i-lucide-video', mkv: 'i-lucide-video',
  mp3: 'i-lucide-music', wav: 'i-lucide-music', flac: 'i-lucide-music', ogg: 'i-lucide-music',
  pdf: 'i-lucide-file-text',
  zip: 'i-lucide-archive', tar: 'i-lucide-archive', gz: 'i-lucide-archive',
  '7z': 'i-lucide-archive', rar: 'i-lucide-archive',
}

const rtf = new Intl.RelativeTimeFormat(undefined, { numeric: 'auto' })

export function getFileIcon(name: string): string {
  const ext = name.split('.').pop()?.toLowerCase() || ''
  return FILE_ICONS[ext] || 'i-lucide-file'
}

export function formatDate(timestamp: bigint | number | null): string {
  if (!timestamp) return ''
  const ms = typeof timestamp === 'bigint' ? Number(timestamp) * 1000 : Number(timestamp) * 1000
  const diff = Date.now() - ms
  const seconds = Math.floor(diff / 1000)

  if (seconds < 60) return rtf.format(-seconds, 'second')
  const minutes = Math.floor(seconds / 60)
  if (minutes < 60) return rtf.format(-minutes, 'minute')
  const hours = Math.floor(minutes / 60)
  if (hours < 24) return rtf.format(-hours, 'hour')
  const days = Math.floor(hours / 24)
  if (days < 30) return rtf.format(-days, 'day')
  const months = Math.floor(days / 30)
  if (months < 12) return rtf.format(-months, 'month')
  return rtf.format(-Math.floor(months / 12), 'year')
}

export const isContentUri = (p: string) => p.startsWith('{')

/**
 * Convert internal `/`-prefixed currentPath to an S3 prefix string.
 *   `/`           → `""`           (bucket root)
 *   `/dir/sub`    → `"dir/sub/"`   (folder)
 * S3 keys never start with `/` and folder prefixes always end with `/`.
 */
export const toS3Prefix = (path: string): string => {
  if (path === '/' || path === '') return ''
  const stripped = path.replace(/^\/+/, '').replace(/\/+$/, '')
  return stripped ? `${stripped}/` : ''
}

/**
 * Stable hash of `(backendId, key)` used to name on-disk cache files for
 * the streaming-via-cache play path. Uses Web Crypto SHA-256 so we don't
 * pull a hashing dep just for this. Hex-truncated to keep paths readable.
 */
export async function cacheKeyHash(backendId: string, key: string): Promise<string> {
  const data = new TextEncoder().encode(`${backendId} ${key}`)
  const digest = await crypto.subtle.digest('SHA-256', data)
  return Array.from(new Uint8Array(digest))
    .slice(0, 16)
    .map((b) => b.toString(16).padStart(2, '0'))
    .join('')
}

/**
 * Prompt for a save location and write `base64` there.
 *
 * Used for local file downloads (small enough that the base64 round-trip
 * through the Tauri IPC is fine). S3 downloads use the dedicated chunked
 * resumable path in `streamingDownloadToPath()` instead — that one streams
 * directly from S3 to disk in Rust without round-tripping a base64 string.
 *
 * Returns false if the user cancels the save dialog.
 */
export async function saveBase64WithDialog(
  base64: string,
  defaultFilename: string,
): Promise<boolean> {
  const target = await showSaveDialog({ defaultPath: defaultFilename })
  if (!target) return false

  let bytes: Uint8Array
  try {
    const binary = atob(base64)
    bytes = new Uint8Array(binary.length)
    for (let i = 0; i < binary.length; i++) bytes[i] = binary.charCodeAt(i)
  } catch (e) {
    console.error('[saveBase64WithDialog] failed to decode base64:', e)
    return false
  }
  await writeFile(target, bytes)
  return true
}
