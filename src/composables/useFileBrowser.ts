import { invoke } from '@tauri-apps/api/core'
import type { StorageListDirResponse } from '~/../src-tauri/bindings/StorageListDirResponse'
import {
  type FileEntry,
  type RemotePeer,
  type ViewMode,
  type GlobalSearchFile,
  CHUNK_SIZE,
  getFileIcon,
  formatDate,
  isContentUri,
  toS3Prefix,
} from '~/composables/fileBrowserHelpers'
import { readableFileSize } from '~/utils/helper'

export function useFileBrowser(tabId: string) {
  const peerStore = usePeerStorageStore()
  const preview = useFilePreview()
  const clipboard = useFileClipboard()

  const selectedPeer = ref<RemotePeer | null>(null)
  const currentPath = ref('/')
  // Origin spaceId for the current navigation. Set when the user clicks an
  // entry returned by `peerStore.remoteListAllSharesAsync` (which tags every
  // entry with its origin space), so subsequent peer-storage calls keep
  // addressing the same space even when share names collide across spaces.
  // Cleared whenever we return to root or switch peers.
  const currentSpaceId = ref<string | null>(null)
  const files = ref<FileEntry[]>([])
  const totalFiles = ref(0)
  const isLoading = ref(false)
  const isLoadingMore = ref(false)
  const loadError = ref<string | null>(null)
  const direction = ref<'forward' | 'back'>('forward')

  // Navigation stack for Android Content URI support.
  // Each entry stores the display name and the actual path/URI to navigate to.
  // On desktop, currentPath alone is sufficient, but on Android we need
  // the full Content URI per level since URIs can't be reconstructed from names.
  const navStack = ref<{ name: string; path: string }[]>([])
  // Forward stack tracks not just the path but also the origin spaceId, so
  // navigating forward into a previously-visited share resumes the same
  // space context (matters when two spaces shared with the same peer have
  // colliding share names — otherwise the resolver falls back to the
  // by-name lookup and may pick the wrong space).
  const forwardStack = ref<{
    path: string
    navStack: { name: string; path: string }[]
    spaceId: string | null
  }[]>([])

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

  // Shallow + deep + global fuzzy search. `resetSearch` is called by
  // `loadFiles` so a fresh listing never shows stale deep-search results.
  const {
    searchQuery,
    isSearching,
    isGlobalSearching,
    filteredFiles,
    filteredGlobalFiles,
    resetSearch,
  } = useFileSearch({ selectedPeer, currentPath, currentSpaceId, sortedFiles })

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

  // S3 chunked-transfer progress + lifecycle (download/upload/cancel).
  const {
    getS3TransferProgress,
    cancelFileTransferAsync,
    startS3ChunkedDownload,
    startS3ChunkedUpload,
  } = useS3Transfers({ selectedPeer, currentPath, resolveFilePath })

  // Media playback / inline preview (audio/video stream via the range
  // server, image/pdf preview, thumbnails).
  const { canPlayFile, playFile, getThumbnailUrl } = useMediaPlayback({
    selectedPeer,
    currentPath,
    currentSpaceId,
    preview,
    resolveFilePath,
    resolveLocalAbsolutePath,
    startS3ChunkedDownload,
  })

  // =========================================================================
  // Navigation
  // =========================================================================

  const selectPeer = (peer: RemotePeer, initialPath = '/') => {
    clearSelection()
    direction.value = 'forward'
    selectedPeer.value = peer
    currentPath.value = initialPath
    currentSpaceId.value = null
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
    currentSpaceId.value = null
    navStack.value = []
    forwardStack.value = []
    files.value = []
  }

  /** Save current location to forward stack before navigating back */
  const pushToForwardStack = () => {
    forwardStack.value = [...forwardStack.value, {
      path: currentPath.value,
      navStack: [...navStack.value],
      spaceId: currentSpaceId.value,
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
    if (currentPath.value === '/') currentSpaceId.value = null
    loadFiles()
  }

  const navigateForward = () => {
    if (forwardStack.value.length === 0) return
    direction.value = 'forward'
    const entry = forwardStack.value[forwardStack.value.length - 1]!
    forwardStack.value = forwardStack.value.slice(0, -1)
    currentPath.value = entry.path
    navStack.value = entry.navStack
    currentSpaceId.value = entry.spaceId
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
    if (currentPath.value === '/') currentSpaceId.value = null
    loadFiles()
  }

  const navigateToPath = (path: string) => {
    direction.value = 'forward'
    currentPath.value = path
    if (path === '/') currentSpaceId.value = null
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
    resetSearch()
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
        // Remote: no pagination (iroh returns all at once). At the root path
        // we need to fan out one request per shared space — see
        // peerStore.remoteListAllSharesAsync for the rationale.
        const result = currentPath.value === '/'
          ? await peerStore.remoteListAllSharesAsync(selectedPeer.value.endpointId)
          : await peerStore.remoteListAsync(
              selectedPeer.value.endpointId,
              currentPath.value,
              currentSpaceId.value ?? undefined,
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
      // Clicking an entry from the multi-space root listing pins navigation
      // to that entry's origin space, so a click on `Photos` in space A
      // never accidentally drops into space B's `Photos`.
      if (currentPath.value === '/' && file.spaceId) {
        currentSpaceId.value = file.spaceId
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

  // Write operations (download / delete / rename / upload / create folder /
  // clipboard) + their backend-support guards.
  const {
    canWrite,
    downloadFile,
    downloadSelectedAsync,
    deleteFile,
    deleteSelectedAsync,
    canDeleteFile,
    renameFile,
    canRenameFile,
    copyFile,
    cutFile,
    canCopyOrCutFile,
    uploadFilesAsync,
    createFolderAsync,
    isCutFile,
    copySelected,
    cutSelected,
    pasteAsync,
    canPaste,
  } = useFileMutations({
    selectedPeer,
    currentPath,
    currentSpaceId,
    files,
    selectedFiles,
    clipboard,
    resolveFilePath,
    resolveLocalAbsolutePath,
    startS3ChunkedDownload,
    startS3ChunkedUpload,
    loadFiles,
    clearSelection,
  })

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
    cancelFileTransferAsync,
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
