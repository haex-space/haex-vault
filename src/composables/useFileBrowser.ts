import { invoke } from '@tauri-apps/api/core'
import type { FileEntry as BaseFileEntry } from '~/../src-tauri/bindings/FileEntry'

// Extended FileEntry with optional path (used on Android for Content URIs)
export type FileEntry = BaseFileEntry & { path?: string }

export interface RemotePeer {
  endpointId: string
  name: string
  source: 'space' | 'contact'
  detail: string
  localPath?: string
}

const CHUNK_SIZE = 50

const FILE_ICONS: Record<string, string> = {
  jpg: 'i-lucide-image', jpeg: 'i-lucide-image', png: 'i-lucide-image',
  gif: 'i-lucide-image', webp: 'i-lucide-image', svg: 'i-lucide-image',
  mp4: 'i-lucide-video', mov: 'i-lucide-video', avi: 'i-lucide-video', mkv: 'i-lucide-video',
  mp3: 'i-lucide-music', wav: 'i-lucide-music', flac: 'i-lucide-music', ogg: 'i-lucide-music',
  pdf: 'i-lucide-file-text',
  zip: 'i-lucide-archive', tar: 'i-lucide-archive', gz: 'i-lucide-archive',
  '7z': 'i-lucide-archive', rar: 'i-lucide-archive',
}

const SIZE_UNITS = ['B', 'KB', 'MB', 'GB']

const rtf = new Intl.RelativeTimeFormat(undefined, { numeric: 'auto' })

function getFileIcon(name: string): string {
  const ext = name.split('.').pop()?.toLowerCase() || ''
  return FILE_ICONS[ext] || 'i-lucide-file'
}

function formatSize(bytes: number | bigint): string {
  const size = typeof bytes === 'bigint' ? Number(bytes) : bytes
  if (size === 0) return '0 B'
  const unitIndex = Math.floor(Math.log(size) / Math.log(1024))
  return `${(size / Math.pow(1024, unitIndex)).toFixed(unitIndex > 0 ? 1 : 0)} ${SIZE_UNITS[unitIndex]}`
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

export function useFileBrowser() {
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
    selectedFiles.value = new Set(files.value.map(f => f.name))
  }

  const clearSelection = () => {
    selectedFiles.value = new Set()
  }

  const selectionCount = computed(() => selectedFiles.value.size)
  const allSelected = computed(() => files.value.length > 0 && selectedFiles.value.size === files.value.length)

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
  // Path helpers
  // =========================================================================

  const isContentUri = (p: string) => p.startsWith('{')

  const resolveFilePath = (file: FileEntry) => {
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

  const selectPeer = (peer: RemotePeer) => {
    clearSelection()
    selectedPeer.value = peer
    currentPath.value = '/'
    navStack.value = []
    forwardStack.value = []
    loadFiles()

    // Register back action so back button returns to device list
    pushBack({ undo: () => navigateToRoot() })
  }

  const navigateToRoot = () => {
    clearSelection()
    preview.close()
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
    clearSelection()

    const generation = ++loadGeneration

    try {
      if (selectedPeer.value.localPath) {
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

  const { pushBack } = useBackNavigation()

  const onFileClick = async (file: FileEntry) => {
    if (file.isDir) {
      forwardStack.value = [] // New navigation clears forward history
      const filePath = resolveFilePath(file)
      // Android: track Content URI navigation in navStack for correct breadcrumbs / back-navigation
      if (file.path && isContentUri(file.path)) {
        navStack.value = [...navStack.value, { name: file.name, path: file.path }]
      }
      currentPath.value = filePath
      loadFiles()

      // Register back action so back button/gesture navigates up
      pushBack({ undo: () => navigateUp() })
      return
    }

    // Files: download (if remote) then open with system app.
    // No inline preview — avoids downloading entire files just to show them in-app.
    if (selectedPeer.value?.localPath) {
      // Local share: open directly with system app
      const absPath = resolveLocalAbsolutePath(file)
      if (absPath) await preview.openWithSystem(absPath)
    } else {
      // Remote peer: download to cache, then open with system app
      const localPath = await peerStore.remoteReadAsync(
        selectedPeer.value!.endpointId,
        resolveFilePath(file),
      )
      await preview.openWithSystem(localPath)
    }
  }

  const downloadFile = async (file: FileEntry) => {
    if (!selectedPeer.value) return

    if (selectedPeer.value.localPath) {
      // Local file: read as base64 and trigger browser download
      const absPath = resolveLocalAbsolutePath(file)
      if (!absPath) return
      const base64 = await invoke<string>('filesystem_read_file', { path: absPath })
      preview.downloadBase64(base64, file.name)
    } else {
      // Remote file: download directly to disk via streaming
      await peerStore.remoteReadAsync(
        selectedPeer.value.endpointId,
        resolveFilePath(file),
      )
      // File is now in cache dir — nothing to do in the browser
    }
  }

  const downloadSelectedAsync = async () => {
    const selected = files.value.filter(f => selectedFiles.value.has(f.name) && !f.isDir)
    for (const file of selected) {
      await downloadFile(file)
    }
  }

  const deleteSelectedAsync = async () => {
    if (!selectedPeer.value?.localPath) return
    const selected = files.value.filter(f => selectedFiles.value.has(f.name))
    for (const file of selected) {
      const filePath = resolveFilePath(file)
      const fullPath = `${selectedPeer.value.localPath}/${filePath.replace(/^\//, '')}`
      await invoke('filesystem_remove', { path: fullPath })
    }
    clearSelection()
    await loadFiles()
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

  const canPaste = computed(() => clipboard.hasClipboard.value && !!selectedPeer.value?.localPath)

  return {
    // State
    selectedPeer: readonly(selectedPeer),
    currentPath: readonly(currentPath),
    files: readonly(files),
    sortedFiles,
    totalFiles: readonly(totalFiles),
    hasMore,
    isLoading: readonly(isLoading),
    isLoadingMore: readonly(isLoadingMore),
    loadError: readonly(loadError),
    selectedPeerName,
    pathSegments,
    preview,

    // Navigation
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
    formatSize,
    formatDate,

    // Clipboard
    clipboard,
    canPaste,
    copySelected,
    cutSelected,
    pasteAsync,
    isCutFile,

    // File actions
    onFileClick,
    downloadFile,
    downloadSelectedAsync,
    deleteSelectedAsync,

    // Direct peer setter (for deep-link)
    setInitialPeer: (peer: RemotePeer) => {
      selectedPeer.value = peer
    },
  }
}
