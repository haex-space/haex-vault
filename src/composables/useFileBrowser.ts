import Fuse from 'fuse.js'
import { invoke, convertFileSrc } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { save as showSaveDialog } from '@tauri-apps/plugin-dialog'
import { writeFile, exists as fileExists, mkdir } from '@tauri-apps/plugin-fs'
import { appCacheDir, join as joinPath } from '@tauri-apps/api/path'
import type { FileEntry as BaseFileEntry } from '~/../src-tauri/bindings/FileEntry'
import type { StorageListDirResponse } from '~/../src-tauri/bindings/StorageListDirResponse'
import { getMediaType } from '~/composables/useFilePreview'
import { readableFileSize } from '~/utils/helper'

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
async function saveBase64WithDialog(
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


/**
 * Tauri payload for `storage:transfer:*` events emitted by
 * `remote_storage_download_to_path`. `bytesTotal` is best-effort — some
 * S3-compatible backends don't return Content-Length, in which case the
 * Rust side reports `bytesTotal == bytesDone` (monotone, always 100%).
 */
interface StorageTransferEvent {
  transferId: string
  bytesDone?: number
  bytesTotal?: number
  reason?: string
}

/**
 * Stable hash of `(backendId, key)` used to name on-disk cache files for
 * the streaming-via-cache play path. Uses Web Crypto SHA-256 so we don't
 * pull a hashing dep just for this. Hex-truncated to keep paths readable.
 */
async function cacheKeyHash(backendId: string, key: string): Promise<string> {
  const data = new TextEncoder().encode(`${backendId} ${key}`)
  const digest = await crypto.subtle.digest('SHA-256', data)
  return Array.from(new Uint8Array(digest))
    .slice(0, 16)
    .map((b) => b.toString(16).padStart(2, '0'))
    .join('')
}

// Extended FileEntry with optional path (used on Android for Content URIs)
export type FileEntry = BaseFileEntry & { path?: string }

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

const CHUNK_SIZE = 50

/**
 * Feature flag — whether the iroh P2P transport currently exposes mutation
 * verbs (delete, rename, cross-peer copy). When this flips to `true`, the
 * file browser's per-row context menu starts surfacing those actions for
 * P2P peers, still gated by a write UCAN (`canWrite`). Flip on
 * feat/p2p-mutation-ops once the backend lands.
 */
const P2P_MUTATIONS_SUPPORTED = false

const FILE_ICONS: Record<string, string> = {
  jpg: 'i-lucide-image', jpeg: 'i-lucide-image', png: 'i-lucide-image',
  gif: 'i-lucide-image', webp: 'i-lucide-image', svg: 'i-lucide-image',
  mp4: 'i-lucide-video', mov: 'i-lucide-video', avi: 'i-lucide-video', mkv: 'i-lucide-video',
  mp3: 'i-lucide-music', wav: 'i-lucide-music', flac: 'i-lucide-music', ogg: 'i-lucide-music',
  pdf: 'i-lucide-file-text',
  zip: 'i-lucide-archive', tar: 'i-lucide-archive', gz: 'i-lucide-archive',
  '7z': 'i-lucide-archive', rar: 'i-lucide-archive',
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

const rtf = new Intl.RelativeTimeFormat(undefined, { numeric: 'auto' })

function getFileIcon(name: string): string {
  const ext = name.split('.').pop()?.toLowerCase() || ''
  return FILE_ICONS[ext] || 'i-lucide-file'
}

function formatDate(timestamp: bigint | number | null): string {
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

export function useFileBrowser(tabId: string) {
  const peerStore = usePeerStorageStore()
  const preview = useFilePreview()
  const clipboard = useFileClipboard()

  const selectedPeer = ref<RemotePeer | null>(null)
  const currentPath = ref('/')
  const files = ref<FileEntry[]>([])
  const totalFiles = ref(0)
  const isLoading = ref(false)
  const isLoadingMore = ref(false)
  const loadError = ref<string | null>(null)
  const direction = ref<'forward' | 'back'>('forward')

  // =========================================================================
  // S3 chunked-download progress
  //
  // Mirrors the events emitted by `remote_storage_download_to_path`. Keyed
  // by the S3 key so the row-progress UI can look up by file without
  // needing to know the transfer id. The row only renders during an
  // in-flight transfer — entries are cleared on complete / cancel / fail.
  // =========================================================================
  const s3TransferProgress = ref<Map<string, number>>(new Map())
  // transferId → s3 key so completion / error events can resolve back to
  // the row that started the download.
  const transferIdToKey = new Map<string, string>()
  let progressUnlisteners: UnlistenFn[] = []

  onMounted(async () => {
    const clearByTransferId = (transferId: string) => {
      const key = transferIdToKey.get(transferId)
      if (!key) return
      transferIdToKey.delete(transferId)
      const next = new Map(s3TransferProgress.value)
      next.delete(key)
      s3TransferProgress.value = next
    }

    progressUnlisteners.push(
      await listen<StorageTransferEvent>('storage:transfer:progress', (e) => {
        const key = transferIdToKey.get(e.payload.transferId)
        if (!key) return
        const done = e.payload.bytesDone ?? 0
        const total = e.payload.bytesTotal ?? 0
        const ratio = total > 0 ? Math.min(1, done / total) : 0
        const next = new Map(s3TransferProgress.value)
        next.set(key, ratio)
        s3TransferProgress.value = next
      }),
      await listen<StorageTransferEvent>('storage:transfer:complete', (e) => {
        clearByTransferId(e.payload.transferId)
      }),
      await listen<StorageTransferEvent>('storage:transfer:cancelled', (e) => {
        clearByTransferId(e.payload.transferId)
      }),
      await listen<StorageTransferEvent>('storage:transfer:failed', (e) => {
        clearByTransferId(e.payload.transferId)
      }),
    )
  })

  onUnmounted(() => {
    for (const off of progressUnlisteners) off()
    progressUnlisteners = []
    transferIdToKey.clear()
    s3TransferProgress.value = new Map()
  })

  /**
   * Progress (0..1) of an in-flight S3 chunked download for the file with
   * `fileName` in the current directory, or `undefined` if no transfer is
   * active. Safe to call for non-S3 peers (returns undefined).
   */
  const getS3TransferProgress = (fileName: string): number | undefined => {
    if (!selectedPeer.value?.s3BackendId) return undefined
    const key = toS3Prefix(currentPath.value) + fileName
    return s3TransferProgress.value.get(key)
  }

  /**
   * Start (or resume) a chunked download from S3 to `outputPath`. Returns
   * after the Tauri command resolves — progress events fire in the
   * background and the active S3 row's progress bar reflects them.
   */
  const startS3ChunkedDownload = async (
    backendId: string,
    key: string,
    outputPath: string,
  ): Promise<void> => {
    const transferId = crypto.randomUUID()
    transferIdToKey.set(transferId, key)
    const next = new Map(s3TransferProgress.value)
    next.set(key, 0)
    s3TransferProgress.value = next

    try {
      await invoke('remote_storage_download_to_path', {
        request: { backendId, key, outputPath, transferId },
      })
    } finally {
      // Belt-and-braces — the complete/failed/cancelled event already
      // cleans up, but if it never arrives (extension reload, panic)
      // we still want the row UI to clear.
      transferIdToKey.delete(transferId)
      const cleared = new Map(s3TransferProgress.value)
      cleared.delete(key)
      s3TransferProgress.value = cleared
    }
  }

  const isContentUri = (p: string) => p.startsWith('{')

  /**
   * Convert internal `/`-prefixed currentPath to an S3 prefix string.
   *   `/`           → `""`           (bucket root)
   *   `/dir/sub`    → `"dir/sub/"`   (folder)
   * S3 keys never start with `/` and folder prefixes always end with `/`.
   */
  const toS3Prefix = (path: string): string => {
    if (path === '/' || path === '') return ''
    const stripped = path.replace(/^\/+/, '').replace(/\/+$/, '')
    return stripped ? `${stripped}/` : ''
  }

  // Navigation stack for Android Content URI support.
  // Each entry stores the display name and the actual path/URI to navigate to.
  // On desktop, currentPath alone is sufficient, but on Android we need
  // the full Content URI per level since URIs can't be reconstructed from names.
  const navStack = ref<{ name: string; path: string }[]>([])
  const forwardStack = ref<{ path: string; navStack: { name: string; path: string }[] }[]>([])

  // =========================================================================
  // Multi-select
  // =========================================================================

  const selectedFiles = ref(new Set<string>())

  const isSelected = (file: FileEntry) => selectedFiles.value.has(file.name)

  const toggleSelect = (file: FileEntry) => {
    const next = new Set(selectedFiles.value)
    if (next.has(file.name)) {
      next.delete(file.name)
    } else {
      next.add(file.name)
    }
    selectedFiles.value = next
  }

  const selectAll = () => {
    selectedFiles.value = new Set(filteredFiles.value.map(f => f.name))
  }

  const clearSelection = () => {
    selectedFiles.value = new Set()
  }

  const selectionCount = computed(() => selectedFiles.value.size)
  const allSelected = computed(() => filteredFiles.value.length > 0 && filteredFiles.value.every(f => selectedFiles.value.has(f.name)))

  const selectedPeerName = computed(() => selectedPeer.value?.name || '')

  // Breadcrumb segments: on Android uses navStack names, on desktop splits path
  const pathSegments = computed(() =>
    navStack.value.length > 0
      ? navStack.value.map(s => s.name)
      : currentPath.value.split('/').filter(Boolean),
  )

  // Backend sorts local files already (dirs first, alphabetical).
  // Remote files need frontend sorting.
  const sortedFiles = computed(() => {
    if (selectedPeer.value?.localPath) return files.value
    return [...files.value].sort((a, b) => {
      if (a.isDir !== b.isDir) return a.isDir ? -1 : 1
      return a.name.localeCompare(b.name)
    })
  })

  const hasMore = computed(() => files.value.length < totalFiles.value)

  // =========================================================================
  // View mode & search
  // =========================================================================

  const viewMode = ref<ViewMode>('list')
  const searchQuery = ref('')
  const deepSearchFiles = ref<SearchableFile[]>([])
  const globalSearchFiles = ref<GlobalSearchFile[]>([])
  const isSearching = ref(false)
  const isGlobalSearching = ref(false)
  let searchGeneration = 0
  let searchTimeout: ReturnType<typeof setTimeout> | undefined

  const recursiveSearchAsync = async (basePath: string, generation: number) => {
    // S3 deep search isn't wired up yet — fall back to the shallow filter
    // against `sortedFiles` (handled in `filteredFiles`).
    if (selectedPeer.value?.s3BackendId) return

    interface QueueEntry { path: string; displayPrefix: string }
    const queue: QueueEntry[] = [{ path: basePath, displayPrefix: '' }]
    const results: SearchableFile[] = []

    while (queue.length > 0) {
      if (generation !== searchGeneration) return
      const { path: dirPath, displayPrefix } = queue.shift()!

      try {
        let entries: FileEntry[]
        if (selectedPeer.value?.localPath) {
          const result = await peerStore.localListAsync(selectedPeer.value.localPath, dirPath)
          entries = result.entries as FileEntry[]
        } else {
          entries = await peerStore.remoteListAsync(selectedPeer.value!.endpointId, dirPath)
        }

        for (const entry of entries) {
          const entryPath = entry.path && isContentUri(entry.path)
            ? entry.path
            : dirPath === '/' ? `/${entry.name}` : `${dirPath}/${entry.name}`

          results.push({
            ...entry,
            displayPath: displayPrefix,
            searchPath: entryPath,
          })

          if (entry.isDir) {
            queue.push({
              path: entryPath,
              displayPrefix: displayPrefix ? `${displayPrefix}/${entry.name}` : entry.name,
            })
          }
        }

        if (generation !== searchGeneration) return
        deepSearchFiles.value = [...results]
      } catch {
        // Skip directories that fail to list (permissions, broken links, etc.)
      }
    }
  }

  const globalSearchAsync = async (generation: number) => {
    const shares = peerStore.nodeId
      ? peerStore.shares.filter(s => s.endpointId === peerStore.nodeId)
      : peerStore.shares

    const results: GlobalSearchFile[] = []

    for (const share of shares) {
      if (generation !== searchGeneration) return

      interface QueueEntry { path: string; displayPrefix: string }
      const queue: QueueEntry[] = [{ path: '/', displayPrefix: '' }]

      while (queue.length > 0) {
        if (generation !== searchGeneration) return
        const { path: dirPath, displayPrefix } = queue.shift()!

        try {
          const result = await peerStore.localListAsync(share.localPath, dirPath)
          const entries = result.entries as FileEntry[]

          for (const entry of entries) {
            const entryPath = dirPath === '/' ? `/${entry.name}` : `${dirPath}/${entry.name}`

            results.push({
              ...entry,
              displayPath: displayPrefix ? `${share.name}/${displayPrefix}` : share.name,
              searchPath: entryPath,
              shareId: share.id,
              shareName: share.name,
              shareLocalPath: share.localPath,
            })

            if (entry.isDir) {
              queue.push({
                path: entryPath,
                displayPrefix: displayPrefix ? `${displayPrefix}/${entry.name}` : entry.name,
              })
            }
          }

          if (generation !== searchGeneration) return
          globalSearchFiles.value = [...results]
        } catch {
          // Skip directories that fail to list
        }
      }
    }
  }

  watch(searchQuery, (query) => {
    if (searchTimeout) clearTimeout(searchTimeout)

    if (!query) {
      searchGeneration++
      deepSearchFiles.value = []
      globalSearchFiles.value = []
      isSearching.value = false
      isGlobalSearching.value = false
      return
    }

    searchTimeout = setTimeout(async () => {
      const generation = ++searchGeneration

      if (selectedPeer.value) {
        deepSearchFiles.value = []
        isSearching.value = true
        await recursiveSearchAsync(currentPath.value, generation)
        if (generation === searchGeneration) {
          isSearching.value = false
        }
      } else {
        globalSearchFiles.value = []
        isGlobalSearching.value = true
        await globalSearchAsync(generation)
        if (generation === searchGeneration) {
          isGlobalSearching.value = false
        }
      }
    }, 300)
  })

  const fuse = computed(() => {
    const source = searchQuery.value && deepSearchFiles.value.length > 0
      ? deepSearchFiles.value
      : sortedFiles.value
    return new Fuse(source, { keys: ['name'], threshold: 0.4 })
  })

  const filteredFiles = computed((): (FileEntry & { displayPath?: string; searchPath?: string })[] => {
    if (!searchQuery.value) return sortedFiles.value
    // S3 falls back to shallow search on the currently loaded folder since
    // there is no deep-search backend wired up yet.
    if (selectedPeer.value?.s3BackendId) return fuse.value.search(searchQuery.value).map(r => r.item)
    if (deepSearchFiles.value.length === 0) return []
    return fuse.value.search(searchQuery.value).map(r => r.item)
  })

  const globalFuse = computed(() => {
    return new Fuse(globalSearchFiles.value, { keys: ['name'], threshold: 0.4 })
  })

  const filteredGlobalFiles = computed((): GlobalSearchFile[] => {
    if (!searchQuery.value || globalSearchFiles.value.length === 0) return []
    return globalFuse.value.search(searchQuery.value).map(r => r.item)
  })

  // =========================================================================
  // Path helpers
  // =========================================================================

  const resolveFilePath = (file: FileEntry & { searchPath?: string }) => {
    // Deep search result: use stored absolute path
    if (file.searchPath) return file.searchPath
    // Android Content URI: use the full path from the DirEntry
    if (file.path && isContentUri(file.path)) return file.path
    return currentPath.value === '/'
      ? `/${file.name}`
      : `${currentPath.value}/${file.name}`
  }

  const resolveLocalAbsolutePath = (file: FileEntry) => {
    if (!selectedPeer.value?.localPath) return null
    // Android Content URI: file.path IS the absolute path
    if (file.path && isContentUri(file.path)) return file.path
    return `${selectedPeer.value.localPath}/${resolveFilePath(file).replace(/^\//, '')}`
  }

  // =========================================================================
  // Navigation
  // =========================================================================

  const selectPeer = (peer: RemotePeer, initialPath = '/') => {
    clearSelection()
    direction.value = 'forward'
    selectedPeer.value = peer
    currentPath.value = initialPath
    navStack.value = []
    forwardStack.value = []
    loadFiles()

    // Register back action so back button returns to device list
    navigationStore.pushBack({ undo: () => navigateToRoot(), redo: () => selectPeer(peer, initialPath) }, tabId)
  }

  const navigateToRoot = () => {
    clearSelection()
    preview.close()
    direction.value = 'back'
    selectedPeer.value = null
    currentPath.value = '/'
    navStack.value = []
    forwardStack.value = []
    files.value = []
  }

  /** Save current location to forward stack before navigating back */
  const pushToForwardStack = () => {
    forwardStack.value = [...forwardStack.value, {
      path: currentPath.value,
      navStack: [...navStack.value],
    }]
  }

  const navigateUp = () => {
    pushToForwardStack()
    direction.value = 'back'
    if (navStack.value.length > 0) {
      const newStack = navStack.value.slice(0, -1)
      navStack.value = newStack
      currentPath.value = newStack.length > 0 ? newStack[newStack.length - 1]!.path : '/'
    }
    else {
      const segments = pathSegments.value.slice()
      segments.pop()
      currentPath.value = segments.length ? '/' + segments.join('/') : '/'
    }
    loadFiles()
  }

  const navigateForward = () => {
    if (forwardStack.value.length === 0) return
    direction.value = 'forward'
    const entry = forwardStack.value[forwardStack.value.length - 1]!
    forwardStack.value = forwardStack.value.slice(0, -1)
    currentPath.value = entry.path
    navStack.value = entry.navStack
    loadFiles()
  }

  const canGoForward = computed(() => forwardStack.value.length > 0)
  const canGoBack = computed(() => currentPath.value !== '/')

  const navigateToSegment = (index: number) => {
    pushToForwardStack()
    direction.value = 'back'
    if (navStack.value.length > 0) {
      const newStack = navStack.value.slice(0, index + 1)
      navStack.value = newStack
      currentPath.value = newStack[newStack.length - 1]?.path ?? '/'
    }
    else {
      const segments = pathSegments.value.slice(0, index + 1)
      currentPath.value = '/' + segments.join('/')
    }
    loadFiles()
  }

  const navigateToPath = (path: string) => {
    direction.value = 'forward'
    currentPath.value = path
    forwardStack.value = [] // New navigation clears forward history
    loadFiles()
  }

  // =========================================================================
  // Loading
  // =========================================================================

  let loadGeneration = 0

  const loadFiles = async () => {
    if (!selectedPeer.value) return

    // Immediate feedback: clear old list and show spinner
    files.value = []
    totalFiles.value = 0
    isLoading.value = true
    isLoadingMore.value = false
    loadError.value = null
    searchQuery.value = ''
    deepSearchFiles.value = []
    isSearching.value = false
    searchGeneration++
    clearSelection()

    const generation = ++loadGeneration

    try {
      if (selectedPeer.value.s3BackendId) {
        // S3: list a single hierarchy level using the bucket's `/` delimiter.
        const response = await invoke<StorageListDirResponse>(
          'remote_storage_list_dir',
          {
            request: {
              backendId: selectedPeer.value.s3BackendId,
              prefix: toS3Prefix(currentPath.value),
            },
          },
        )
        if (generation !== loadGeneration) return

        const folderEntries: FileEntry[] = response.folders.map((folder) => {
          // Trim the parent prefix to get the folder's basename.
          const parent = toS3Prefix(currentPath.value)
          const name = folder.slice(parent.length).replace(/\/$/, '')
          return { name, size: 0n, isDir: true, modified: null }
        })
        const fileEntries: FileEntry[] = response.objects.map((obj) => {
          const parent = toS3Prefix(currentPath.value)
          const name = obj.key.slice(parent.length)
          // ISO 8601 → unix seconds (matches FileEntry.modified semantics
          // used by formatDate, which multiplies by 1000).
          const modifiedSec = obj.lastModified
            ? BigInt(Math.floor(new Date(obj.lastModified).getTime() / 1000))
            : null
          return {
            name,
            size: BigInt(obj.size),
            isDir: false,
            modified: modifiedSec,
          }
        })

        files.value = [...folderEntries, ...fileEntries]
        totalFiles.value = files.value.length
      } else if (selectedPeer.value.localPath) {
        // Chunked loading for local directories
        const firstChunk = await peerStore.localListAsync(
          selectedPeer.value.localPath,
          currentPath.value,
          0,
          CHUNK_SIZE,
        )

        if (generation !== loadGeneration) return // navigated away
        files.value = firstChunk.entries as FileEntry[]
        totalFiles.value = firstChunk.total
        isLoading.value = false

        // Load remaining chunks in background
        if (firstChunk.total > CHUNK_SIZE) {
          isLoadingMore.value = true
          let offset = CHUNK_SIZE

          while (offset < firstChunk.total) {
            const chunk = await peerStore.localListAsync(
              selectedPeer.value!.localPath!,
              currentPath.value,
              offset,
              CHUNK_SIZE,
            )
            if (generation !== loadGeneration) return
            files.value = [...files.value, ...chunk.entries as FileEntry[]]
            offset += CHUNK_SIZE
          }

          isLoadingMore.value = false
        }
      } else {
        // Remote: no pagination (iroh returns all at once)
        const result = await peerStore.remoteListAsync(
          selectedPeer.value.endpointId,
          currentPath.value,
        )
        if (generation !== loadGeneration) return
        files.value = result
        totalFiles.value = result.length
      }
    } catch (error) {
      if (generation !== loadGeneration) return
      loadError.value = error instanceof Error ? error.message : String(error)
      files.value = []
    } finally {
      if (generation === loadGeneration) {
        isLoading.value = false
        isLoadingMore.value = false
      }
    }
  }

  // =========================================================================
  // File actions
  // =========================================================================

  const navigationStore = useNavigationStore()

  const onFileClick = async (file: FileEntry) => {
    if (file.isDir) {
      forwardStack.value = [] // New navigation clears forward history
      direction.value = 'forward'
      const filePath = resolveFilePath(file)
      // Android: track Content URI navigation in navStack for correct breadcrumbs / back-navigation
      if (file.path && isContentUri(file.path)) {
        navStack.value = [...navStack.value, { name: file.name, path: file.path }]
      }
      currentPath.value = filePath
      loadFiles()

      // Register back action so back button/gesture navigates up
      navigationStore.pushBack({ undo: () => navigateUp(), redo: () => onFileClick(file) }, tabId)
      return
    }

    // Files: route to the unified open path. Media plays inline in the
    // preview modal; everything else is fetched (for remote backends) and
    // handed off to the system handler. The per-row transfer progress bar
    // provides feedback while bytes stream in.
    await playFile(file)
  }

  const onGlobalSearchResultClick = async (file: GlobalSearchFile) => {
    const peer: RemotePeer = {
      endpointId: peerStore.nodeId || '',
      name: file.shareName,
      source: 'space',
      detail: '',
      localPath: file.shareLocalPath,
    }

    if (file.isDir) {
      selectPeer(peer, file.searchPath)
    } else {
      const absPath = `${file.shareLocalPath}/${file.searchPath.replace(/^\//, '')}`
      await preview.openWithSystem(absPath)
    }
  }

  /**
   * Download `file` to a user-chosen destination via the native save dialog.
   *
   * Why not just `<a download>`: that trick is unreliable in Tauri WebViews
   * — WebKitGTK in particular drops the `download` attribute and treats the
   * link as a same-origin navigation. The native `save()` + `writeFile()`
   * round-trip is the only path that works consistently across desktop
   * and Android.
   *
   * Returns false if the user cancelled the save dialog. The caller can
   * treat that as a no-op (no toast needed).
   */
  const downloadFile = async (file: FileEntry): Promise<boolean> => {
    if (!selectedPeer.value) return false

    if (selectedPeer.value.s3BackendId) {
      // Save dialog first, then stream chunk-by-chunk straight to the
      // chosen path — no full file in RAM, automatically resumable if
      // the same target path exists from a previous interrupted run.
      const target = await showSaveDialog({ defaultPath: file.name })
      if (!target) return false
      const key = toS3Prefix(currentPath.value) + file.name
      try {
        await startS3ChunkedDownload(selectedPeer.value.s3BackendId, key, target)
        return true
      } catch (e) {
        // The Rust side surfaces user cancellation as a `DownloadFailed`
        // with reason "cancelled". Treat that as a non-error so toasts
        // don't shout at the user for a deliberate action.
        const msg = e instanceof Error ? e.message : String(e)
        if (msg.includes('cancelled')) return false
        throw e
      }
    } else if (selectedPeer.value.localPath) {
      // Local file: read as base64, then offer save dialog.
      const absPath = resolveLocalAbsolutePath(file)
      if (!absPath) return false
      const base64 = await invoke<string>('filesystem_read_file', { path: absPath })
      return await saveBase64WithDialog(base64, file.name)
    } else {
      // Remote P2P file: existing streaming path already writes to disk
      // cache under the peer-storage's own location.
      await peerStore.remoteReadAsync(
        selectedPeer.value.endpointId,
        resolveFilePath(file),
      )
      return true
    }
  }

  const downloadSelectedAsync = async () => {
    const selected = files.value.filter(f => selectedFiles.value.has(f.name) && !f.isDir)
    for (const file of selected) {
      await downloadFile(file)
    }
  }

  const deleteSelectedAsync = async () => {
    const selected = files.value.filter(f => selectedFiles.value.has(f.name))
    for (const file of selected) {
      await deleteFile(file)
    }
    clearSelection()
    await loadFiles()
  }

  /**
   * Delete a single entry, picking the right transport for the current
   * backend.
   *
   *   - Local: `filesystem_remove` (handles files and directories).
   *   - S3: `remote_storage_delete`. Folders need to be deleted as a "/"-
   *     suffixed object marker, matching how `createFolderAsync` writes them.
   *     S3 doesn't have real directories, so this only removes the marker —
   *     callers should not expect recursive folder deletion until a real
   *     batch API is wired up.
   *   - P2P: not supported by the iroh backend yet — returns early.
   *
   * The caller is responsible for refreshing the listing (`loadFiles()`)
   * if it isn't already chained through `deleteSelectedAsync`.
   */
  const deleteFile = async (file: FileEntry) => {
    if (!selectedPeer.value) return
    if (selectedPeer.value.localPath) {
      const filePath = resolveFilePath(file)
      const fullPath = `${selectedPeer.value.localPath}/${filePath.replace(/^\//, '')}`
      await invoke('filesystem_remove', { path: fullPath })
    } else if (selectedPeer.value.s3BackendId) {
      const prefix = toS3Prefix(currentPath.value)
      const key = file.isDir ? `${prefix}${file.name}/` : prefix + file.name
      await invoke('remote_storage_delete', {
        request: {
          backendId: selectedPeer.value.s3BackendId,
          key,
        },
      })
    } else {
      // P2P delete is unsupported — silently no-op. The caller checks
      // `canDeleteFile` first so this is only reachable via a stale UI.
      return
    }
  }

  /**
   * Whether deleting `file` is supported on the current backend. Mirrors the
   * switch in `deleteFile` — keep them in sync.
   *
   * For P2P peers there are two gates: backend support (`P2P_MUTATIONS_SUPPORTED`,
   * flipped on when the iroh delete verb ships on feat/p2p-mutation-ops)
   * AND a write UCAN (`canWrite`). Surface the menu item only when both
   * are true so users never see a button that the server would reject.
   */
  const canDeleteFile = (_file: FileEntry): boolean => {
    const peer = selectedPeer.value
    if (!peer) return false
    if (peer.localPath) return true
    if (peer.s3BackendId) return true
    return P2P_MUTATIONS_SUPPORTED && canWrite.value
  }

  /**
   * Rename `file` to `newName` (a basename, not a full path). Local shares
   * only — S3 has no native rename (would need copy + delete with a new key),
   * and the P2P transport has no rename verb at all.
   *
   * Returns true on success. Refuses names containing path separators so a
   * rename can never silently move a file across folders.
   */
  const renameFile = async (file: FileEntry, newName: string): Promise<boolean> => {
    const trimmed = newName.trim()
    if (!trimmed || trimmed === file.name) return false
    if (trimmed.includes('/') || trimmed.includes('\\')) return false
    if (!selectedPeer.value?.localPath) return false

    const dir = resolveCurrentDir()
    if (!dir) return false
    const oldPath = `${dir}/${file.name}`
    const newPath = `${dir}/${trimmed}`
    await invoke('filesystem_rename', { from: oldPath, to: newPath })
    await loadFiles()
    return true
  }

  /**
   * Whether `file` can be renamed on the current backend.
   *
   * Local shares only today — S3 has no native rename (would need copy +
   * delete with a new key, tracked on feat/file-ops-extensions) and the
   * P2P transport has no rename verb either. The P2P arm follows the
   * same dual-gate as `canDeleteFile`: backend support + write UCAN.
   */
  const canRenameFile = (_file: FileEntry): boolean => {
    const peer = selectedPeer.value
    if (!peer) return false
    if (peer.localPath) return true
    if (peer.s3BackendId) return false // pending feat/file-ops-extensions
    return P2P_MUTATIONS_SUPPORTED && canWrite.value
  }

  /**
   * Single-file variants of copy/cut. Both delegate to the same `clipboard`
   * store the selection-based copy/cut use, so paste behaviour is identical.
   * Local shares only — clipboard entries need absolute paths and only local
   * shares expose them.
   */
  const copyFile = (file: FileEntry) => {
    const dir = resolveCurrentDir()
    if (!dir) return
    clipboard.copy(dir, [{ name: file.name, isDir: file.isDir, absolutePath: `${dir}/${file.name}` }])
  }

  const cutFile = (file: FileEntry) => {
    const dir = resolveCurrentDir()
    if (!dir) return
    clipboard.cut(dir, [{ name: file.name, isDir: file.isDir, absolutePath: `${dir}/${file.name}` }])
  }

  /**
   * Whether `file` can participate in clipboard copy/cut on the current
   * backend. The clipboard entries need absolute paths (because paste may
   * land on a different share), so today only local shares qualify.
   *
   * P2P arm gated by both backend support (cross-peer copy/cut isn't
   * implemented yet) and a write UCAN — matches the `canDeleteFile` /
   * `canRenameFile` shape so adding the backend is a single flag flip.
   */
  const canCopyOrCutFile = (_file: FileEntry): boolean => {
    const peer = selectedPeer.value
    if (!peer) return false
    if (peer.localPath) return true
    if (peer.s3BackendId) return false // pending feat/file-ops-extensions
    return P2P_MUTATIONS_SUPPORTED && canWrite.value
  }

  /**
   * Whether `file` can be "played" (image/video/audio/pdf preview) on the
   * current backend.
   */
  const canPlayFile = (file: FileEntry): boolean => {
    if (file.isDir) return false
    return getMediaType(file.name) !== 'unsupported'
  }

  /**
   * Materialise an S3 object in the app cache and return the local path.
   *
   * Why we go through disk for the play path even with a `haex-stream://`
   * protocol handler available: WebKitGTK on Linux refuses media loaded
   * through custom URI schemes (the GStreamer media backend only accepts
   * http(s) / file). The detour via `convertFileSrc()` exposes the file
   * via Tauri's asset protocol, which serves over `http://asset.localhost`
   * — a scheme WebKit happily accepts.
   *
   * The download uses the same chunked / resumable Rust command as
   * `downloadFile`, so a previous partial cache file is picked up where
   * it left off.
   */
  const ensureS3FileCachedAsync = async (
    file: FileEntry,
    backendId: string,
  ): Promise<string> => {
    const key = toS3Prefix(currentPath.value) + file.name
    const cacheRoot = await appCacheDir()
    const subdir = await joinPath(cacheRoot, 's3-stream-cache')
    if (!(await fileExists(subdir))) {
      await mkdir(subdir, { recursive: true })
    }
    const hash = await cacheKeyHash(backendId, key)
    const cachePath = await joinPath(subdir, `${hash}-${file.name}`)
    await startS3ChunkedDownload(backendId, key, cachePath)
    return cachePath
  }

  /**
   * Unified open path for a file. Called both from single-click and the
   * "Play" context-menu item.
   *
   * Media (image/video/audio/pdf) opens in the inline preview modal so the
   * user gets immediate feedback inside the app. Everything else is handed
   * to the OS via `openWithSystem`.
   *
   * Per backend:
   *   - S3 audio/video: chunk-streamed into the app cache and exposed via a
   *     local HTTP range server (`media_server_register`). WebKitGTK's
   *     GStreamer pipeline rejects `asset://` URLs on Linux, so the plain
   *     `http://127.0.0.1:<port>/…` URL is the only thing that plays back
   *     reliably across platforms.
   *   - S3 other: chunk-streamed to cache, then `openLocal` (image/pdf via
   *     `convertFileSrc`) or `openWithSystem` (everything else).
   *   - Local share: direct `openLocal` / `openWithSystem` on the share path.
   *   - P2P peer: `remoteReadAsync` to materialise into the peer cache, then
   *     `openLocal` / `openWithSystem`.
   *
   * The chunked S3 downloads are resumable, so a cancelled / interrupted
   * fetch is picked up by the next click instead of restarting from zero.
   */
  const playFile = async (file: FileEntry) => {
    const peer = selectedPeer.value
    if (!peer) return
    const type = getMediaType(file.name)
    const isMedia = type !== 'unsupported'

    if (peer.s3BackendId && (type === 'audio' || type === 'video')) {
      try {
        const key = toS3Prefix(currentPath.value) + file.name
        const url = await invoke<string>('media_server_register_s3_stream', {
          backendId: peer.s3BackendId,
          key,
        })
        preview.openStream(url, file.name)
        return
      } catch (e) {
        const msg = e instanceof Error ? e.message : String(e)
        if (msg.includes('cancelled')) return
        throw e
      }
    }

    // P2P audio/video: stream over iroh range reads via the local media
    // server, no full-file download to disk first. UCAN + relayUrl come
    // from the same resolver the rest of the peer-storage API uses, so
    // capability checks stay in lock-step with `remoteReadAsync`.
    if (!peer.s3BackendId && !peer.localPath && (type === 'audio' || type === 'video')) {
      const path = resolveFilePath(file)
      const { ucanToken, relayUrl: deviceRelayUrl } =
        peerStore.resolveRequestContext(peer.endpointId, path)
      if (!ucanToken) {
        throw new Error('No valid UCAN token for this peer\'s space')
      }
      try {
        const url = await invoke<string>('media_server_register_peer_stream', {
          nodeId: peer.endpointId,
          relayUrl: deviceRelayUrl,
          path,
          ucanToken,
        })
        preview.openStream(url, file.name)
        return
      } catch (e) {
        const msg = e instanceof Error ? e.message : String(e)
        if (msg.includes('cancelled')) return
        throw e
      }
    }

    let absPath: string | null = null
    try {
      if (peer.s3BackendId) {
        absPath = await ensureS3FileCachedAsync(file, peer.s3BackendId)
      } else if (peer.localPath) {
        absPath = resolveLocalAbsolutePath(file)
      } else {
        absPath = await peerStore.remoteReadAsync(
          peer.endpointId,
          resolveFilePath(file),
        )
      }
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e)
      if (msg.includes('cancelled')) return
      throw e
    }
    if (!absPath) return

    if (isMedia) {
      await preview.openLocal(absPath, file.name, file.size)
    } else {
      await preview.openWithSystem(absPath)
    }
  }

  // =========================================================================
  // Upload / create folder
  // =========================================================================

  /**
   * Whether the currently selected peer supports adding files / folders
   * *from this user*. The server still enforces the same check — this is
   * only a UI gate so users don't see write buttons for shares they have
   * read-only access to.
   *
   *  - Local share: the share lives on this device, the OS enforces perms.
   *  - S3 backend: the user configured it; ACL enforcement is on the bucket.
   *  - P2P peer: requires a cached UCAN with `space/write` or `space/admin`.
   */
  const canWrite = computed(() => {
    const peer = selectedPeer.value
    if (!peer) return false
    if (peer.localPath || peer.s3BackendId) return true
    const cap = peerStore.getCapabilityForPeer(peer.endpointId, currentPath.value)
    return cap === 'space/write' || cap === 'space/admin'
  })

  /** Join the current path with a basename, preserving the `/dir/sub` form. */
  const joinRemotePath = (name: string): string =>
    currentPath.value === '/' ? `/${name}` : `${currentPath.value}/${name}`

  /**
   * Open the native file picker and add the chosen file(s) to the current
   * folder. Supports local shares (zero-copy via `filesystem_copy`), S3
   * backends (base64 round-trip via `remote_storage_upload`), and remote
   * P2P peers (streamed via the iroh `Request::Write` protocol).
   * Returns the number of files added so the caller can show a toast.
   */
  const uploadFilesAsync = async (): Promise<number> => {
    if (!canWrite.value || !selectedPeer.value) return 0

    const selected = await invoke<string[] | null>('filesystem_select_file', {
      multiple: true,
    })
    if (!selected || selected.length === 0) return 0

    const basename = (p: string): string => {
      const parts = p.split(/[/\\]/)
      return parts[parts.length - 1] || p
    }

    if (selectedPeer.value.localPath) {
      const targetDir = resolveCurrentDir()
      if (!targetDir) return 0
      for (const src of selected) {
        await invoke('filesystem_copy', {
          from: src,
          to: `${targetDir}/${basename(src)}`,
        })
      }
    } else if (selectedPeer.value.s3BackendId) {
      const prefix = toS3Prefix(currentPath.value)
      for (const src of selected) {
        const data = await invoke<string>('filesystem_read_file', { path: src })
        await invoke('remote_storage_upload', {
          request: {
            backendId: selectedPeer.value.s3BackendId,
            key: prefix + basename(src),
            data,
          },
        })
      }
    } else {
      // Remote P2P peer: read on Rust side and stream over iroh.
      for (const src of selected) {
        await peerStore.remoteWriteAsync(
          selectedPeer.value.endpointId,
          joinRemotePath(basename(src)),
          src,
        )
      }
    }

    await loadFiles()
    return selected.length
  }

  /**
   * Create a new folder under the current path. For S3 this uploads a
   * zero-byte object with a trailing `/`, which is the conventional way to
   * surface an "empty folder" through the delimiter-based listing. For
   * P2P peers it sends a `Request::CreateDirectory` over iroh.
   * Returns true on success.
   */
  const createFolderAsync = async (rawName: string): Promise<boolean> => {
    if (!canWrite.value || !selectedPeer.value) return false
    const name = rawName.trim().replace(/^\/+|\/+$/g, '')
    // Refuse names with path separators — folder creation is a single-level
    // operation; nested paths would silently surprise the user.
    if (!name || name.includes('/') || name.includes('\\')) return false

    if (selectedPeer.value.localPath) {
      const targetDir = resolveCurrentDir()
      if (!targetDir) return false
      await invoke('filesystem_mkdir', { path: `${targetDir}/${name}` })
    } else if (selectedPeer.value.s3BackendId) {
      const prefix = toS3Prefix(currentPath.value)
      await invoke('remote_storage_upload', {
        request: {
          backendId: selectedPeer.value.s3BackendId,
          key: `${prefix}${name}/`,
          data: '',
        },
      })
    } else {
      await peerStore.remoteCreateDirectoryAsync(
        selectedPeer.value.endpointId,
        joinRemotePath(name),
      )
    }

    await loadFiles()
    return true
  }

  // =========================================================================
  // Clipboard operations
  // =========================================================================

  const resolveCurrentDir = () => {
    if (!selectedPeer.value?.localPath) return null
    const sub = currentPath.value === '/' ? '' : currentPath.value.replace(/^\//, '')
    return sub ? `${selectedPeer.value.localPath}/${sub}` : selectedPeer.value.localPath
  }

  const isCutFile = (file: FileEntry) => {
    const dir = resolveCurrentDir()
    if (!dir) return false
    return clipboard.cutPaths.value.has(`${dir}/${file.name}`)
  }

  const buildClipboardEntries = () => {
    const dir = resolveCurrentDir()
    if (!dir) return []
    return files.value
      .filter(f => selectedFiles.value.has(f.name))
      .map(f => ({
        name: f.name,
        isDir: f.isDir,
        absolutePath: `${dir}/${f.name}`,
      }))
  }

  const copySelected = () => {
    const entries = buildClipboardEntries()
    if (entries.length === 0) return
    clipboard.copy(resolveCurrentDir()!, entries)
    clearSelection()
  }

  const cutSelected = () => {
    const entries = buildClipboardEntries()
    if (entries.length === 0) return
    clipboard.cut(resolveCurrentDir()!, entries)
    clearSelection()
  }

  const pasteAsync = async () => {
    const targetDir = resolveCurrentDir()
    if (!targetDir) return
    await clipboard.pasteAsync(targetDir)
    await loadFiles()
  }

  /**
   * Whether the clipboard's contents can be pasted into the current folder.
   * Requires (a) something on the clipboard and (b) the active peer to
   * accept writes. P2P follows the same dual-gate shape as the other
   * mutation helpers — backend support + write UCAN.
   */
  const canPaste = computed(() => {
    if (!clipboard.hasClipboard.value) return false
    const peer = selectedPeer.value
    if (!peer) return false
    if (peer.localPath) return true
    if (peer.s3BackendId) return false // pending feat/file-ops-extensions
    return P2P_MUTATIONS_SUPPORTED && canWrite.value
  })

  // =========================================================================
  // Thumbnails
  // =========================================================================

  const getThumbnailUrl = (file: FileEntry): string | null => {
    if (file.isDir) return null
    if (getMediaType(file.name) !== 'image') return null
    if (!selectedPeer.value?.localPath) return null
    const absPath = resolveLocalAbsolutePath(file)
    if (!absPath || absPath.startsWith('{')) return null
    return convertFileSrc(absPath)
  }

  return {
    // State
    selectedPeer: readonly(selectedPeer),
    currentPath: readonly(currentPath),
    files: readonly(files),
    sortedFiles,
    filteredFiles,
    totalFiles: readonly(totalFiles),
    hasMore,
    isLoading: readonly(isLoading),
    isLoadingMore: readonly(isLoadingMore),
    loadError: readonly(loadError),
    selectedPeerName,
    pathSegments,
    preview,

    // View mode & search
    viewMode,
    searchQuery,
    isSearching: readonly(isSearching),
    isGlobalSearching: readonly(isGlobalSearching),
    filteredGlobalFiles,

    // Thumbnails
    getThumbnailUrl,

    // Navigation
    direction: readonly(direction),
    selectPeer,
    navigateToRoot,
    navigateUp,
    navigateForward,
    navigateToSegment,
    navigateToPath,
    canGoBack,
    canGoForward,
    loadFiles,

    // Selection
    selectedFiles: readonly(selectedFiles),
    selectionCount,
    allSelected,
    isSelected,
    toggleSelect,
    selectAll,
    clearSelection,

    // Formatters
    getFileIcon,
    formatSize: readableFileSize,
    formatDate,

    // Clipboard
    clipboard,
    canPaste,
    copySelected,
    cutSelected,
    pasteAsync,
    isCutFile,
    copyFile,
    cutFile,
    canCopyOrCutFile,

    // File actions
    onFileClick,
    onGlobalSearchResultClick,
    downloadFile,
    downloadSelectedAsync,
    getS3TransferProgress,
    deleteFile,
    deleteSelectedAsync,
    canDeleteFile,
    renameFile,
    canRenameFile,
    playFile,
    canPlayFile,
    uploadFilesAsync,
    createFolderAsync,
    canWrite,

    // Direct peer setter (for deep-link)
    setInitialPeer: (peer: RemotePeer) => {
      selectedPeer.value = peer
    },
  }
}
