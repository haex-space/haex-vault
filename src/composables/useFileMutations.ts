import { invoke } from '@tauri-apps/api/core'
import { save as showSaveDialog } from '@tauri-apps/plugin-dialog'
import type { Ref } from 'vue'
import {
  toS3Prefix,
  saveBase64WithDialog,
  P2P_MUTATIONS_SUPPORTED,
  type FileEntry,
  type RemotePeer,
} from '~/composables/fileBrowserHelpers'

interface FileMutationsDeps {
  selectedPeer: Ref<RemotePeer | null>
  currentPath: Ref<string>
  currentSpaceId: Ref<string | null>
  files: Ref<FileEntry[]>
  selectedFiles: Ref<Set<string>>
  clipboard: ReturnType<typeof useFileClipboard>
  resolveFilePath: (file: FileEntry & { searchPath?: string }) => string
  resolveLocalAbsolutePath: (file: FileEntry) => string | null
  startS3ChunkedDownload: (backendId: string, key: string, outputPath: string) => Promise<void>
  startS3ChunkedUpload: (backendId: string, key: string, sourcePath: string) => Promise<void>
  loadFiles: () => Promise<void>
  clearSelection: () => void
}

/**
 * Write operations for the file browser: download, delete, rename, upload,
 * create folder, and clipboard copy/cut/paste. Each picks the right
 * transport for the active backend (local / S3 / P2P) and most refresh the
 * listing via `loadFiles` afterwards. The `can*` guards mirror each
 * operation's backend support so the UI never offers an action the server
 * would reject.
 */
export function useFileMutations(deps: FileMutationsDeps) {
  const {
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
  } = deps
  const peerStore = usePeerStorageStore()

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
    const cap = peerStore.getCapabilityForPeer(
      peer.endpointId,
      currentPath.value,
      currentSpaceId.value ?? undefined,
    )
    return cap === 'space/write' || cap === 'space/admin'
  })

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
        undefined,
        file.spaceId ?? currentSpaceId.value ?? undefined,
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

  /** Join the current path with a basename, preserving the `/dir/sub` form. */
  const joinRemotePath = (name: string): string =>
    currentPath.value === '/' ? `/${name}` : `${currentPath.value}/${name}`

  /**
   * Open the native file picker and add the chosen file(s) to the current
   * folder. Supports local shares (zero-copy via `filesystem_copy`), S3
   * backends (streamed via `remote_storage_upload_from_path` so multi-GB
   * files don't have to fit in IPC memory), and remote P2P peers (streamed
   * via the iroh `Request::Write` protocol with progress + cancellation).
   *
   * Each remote upload inserts a placeholder row into `files.value` so the
   * existing per-row progress + X-cancel UI works while the transfer runs;
   * the placeholder is replaced by the real listing once `loadFiles()`
   * refreshes at the end. Cancelled or failed uploads have their placeholder
   * removed in-line so they don't linger before the refresh.
   *
   * Returns the number of files the caller asked to upload (regardless of
   * how many ultimately succeeded — the caller's toast just acknowledges
   * the intent).
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

    const isCancelledError = (e: unknown): boolean => {
      const msg = e instanceof Error ? e.message : String(e)
      return msg.includes('cancelled')
    }

    // Track placeholders so removal can't ever drop a real listing entry
    // that happens to share a name. Belt-and-braces because the placeholder
    // has `modified: null` and isDir false which *should* be unambiguous,
    // but explicit identity is cheaper than reasoning about that.
    const placeholders = new Set<{ name: string; size: bigint; isDir: false; modified: null }>()
    const insertPlaceholder = (name: string) => {
      const entry = { name, size: 0n, isDir: false as const, modified: null }
      placeholders.add(entry)
      files.value = [...files.value, entry]
    }
    const removePlaceholder = (name: string) => {
      for (const entry of placeholders) {
        if (entry.name === name) {
          placeholders.delete(entry)
          files.value = files.value.filter(f => f !== entry)
          return
        }
      }
    }

    // Sequential `for await` is deliberate: each upload runs against one
    // transferId / one cancel token, and the placeholder UX (one progress
    // bar per row) is easier to follow when files complete in order. Fan-out
    // would also need a per-row queue indicator. The file-sync provider
    // path (cloud_provider.rs) already does its own parallel multipart for
    // bulk syncs.
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
        const name = basename(src)
        insertPlaceholder(name)
        try {
          await startS3ChunkedUpload(
            selectedPeer.value.s3BackendId,
            prefix + name,
            src,
          )
        } catch (e) {
          removePlaceholder(name)
          if (!isCancelledError(e)) throw e
        }
      }
    } else {
      // Remote P2P peer: read on Rust side and stream over iroh.
      for (const src of selected) {
        const name = basename(src)
        insertPlaceholder(name)
        try {
          await peerStore.remoteWriteAsync(
            selectedPeer.value.endpointId,
            joinRemotePath(name),
            src,
            currentSpaceId.value ?? undefined,
          )
        } catch (e) {
          removePlaceholder(name)
          if (!isCancelledError(e)) throw e
        }
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
        currentSpaceId.value ?? undefined,
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

  return {
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
  }
}
