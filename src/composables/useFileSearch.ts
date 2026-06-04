import Fuse from 'fuse.js'
import type { ComputedRef, Ref } from 'vue'
import {
  isContentUri,
  type FileEntry,
  type RemotePeer,
  type SearchableFile,
  type GlobalSearchFile,
} from '~/composables/fileBrowserHelpers'

interface FileSearchDeps {
  selectedPeer: Ref<RemotePeer | null>
  currentPath: Ref<string>
  currentSpaceId: Ref<string | null>
  sortedFiles: ComputedRef<FileEntry[]>
}

/**
 * Search for the file browser: shallow fuzzy filter of the current folder,
 * deep recursive search within the selected peer, and global search across
 * all local shares when no peer is selected. A monotonic `generation`
 * counter cancels stale in-flight walks when the query changes or a load
 * resets the view.
 */
export function useFileSearch(deps: FileSearchDeps) {
  const { selectedPeer, currentPath, currentSpaceId, sortedFiles } = deps
  const peerStore = usePeerStorageStore()

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

    // Queue carries `spaceId` so subdirectory listings stay scoped to the
    // share's origin space. Without this the root fan-out's per-entry
    // spaceId gets dropped on the first descend, and colliding share names
    // across spaces collapse onto whichever space the by-name fallback
    // picks first — missing one space and duplicating the other.
    interface QueueEntry { path: string; displayPrefix: string; spaceId?: string }
    const queue: QueueEntry[] = [{
      path: basePath,
      displayPrefix: '',
      spaceId: currentSpaceId.value ?? undefined,
    }]
    const results: SearchableFile[] = []

    while (queue.length > 0) {
      if (generation !== searchGeneration) return
      const { path: dirPath, displayPrefix, spaceId } = queue.shift()!

      try {
        let entries: FileEntry[]
        if (selectedPeer.value?.localPath) {
          const result = await peerStore.localListAsync(selectedPeer.value.localPath, dirPath)
          entries = result.entries as FileEntry[]
        } else if (dirPath === '/') {
          entries = await peerStore.remoteListAllSharesAsync(selectedPeer.value!.endpointId)
        } else {
          entries = await peerStore.remoteListAsync(
            selectedPeer.value!.endpointId,
            dirPath,
            spaceId,
          )
        }

        for (const entry of entries) {
          const entryPath = entry.path && isContentUri(entry.path)
            ? entry.path
            : dirPath === '/' ? `/${entry.name}` : `${dirPath}/${entry.name}`

          results.push({
            ...entry,
            spaceId: entry.spaceId ?? spaceId,
            displayPath: displayPrefix,
            searchPath: entryPath,
          })

          if (entry.isDir) {
            queue.push({
              path: entryPath,
              displayPrefix: displayPrefix ? `${displayPrefix}/${entry.name}` : entry.name,
              spaceId: entry.spaceId ?? spaceId,
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

  /**
   * Synchronously clear the active search. Called by `loadFiles` so a new
   * listing never shows stale deep-search results — `searchQuery` is also
   * watched, but that callback flushes asynchronously, so we reset here too.
   */
  const resetSearch = () => {
    searchQuery.value = ''
    deepSearchFiles.value = []
    isSearching.value = false
    searchGeneration++
  }

  return {
    searchQuery,
    isSearching,
    isGlobalSearching,
    filteredFiles,
    filteredGlobalFiles,
    resetSearch,
  }
}
