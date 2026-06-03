import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import type { Ref } from 'vue'
import {
  toS3Prefix,
  type FileEntry,
  type RemotePeer,
} from '~/composables/fileBrowserHelpers'

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

interface S3TransferDeps {
  selectedPeer: Ref<RemotePeer | null>
  currentPath: Ref<string>
  resolveFilePath: (file: FileEntry & { searchPath?: string }) => string
}

/**
 * S3 chunked-transfer progress + lifecycle for the file browser.
 *
 * Mirrors the events emitted by `remote_storage_download_to_path`. Progress
 * is keyed by the S3 key so the row-progress UI can look up by file without
 * needing to know the transfer id. The row only renders during an in-flight
 * transfer — entries are cleared on complete / cancel / fail.
 *
 * Both download and upload reuse the same `transferIdToKey` +
 * `s3TransferProgress` maps so the per-row UI (progress overlay + X cancel
 * button) lights up identically in either direction.
 */
export function useS3Transfers(deps: S3TransferDeps) {
  const { selectedPeer, currentPath, resolveFilePath } = deps
  const peerStore = usePeerStorageStore()

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

  const getS3TransferIdForKey = (key: string): string | undefined => {
    for (const [transferId, entryKey] of transferIdToKey.entries()) {
      if (entryKey === key) return transferId
    }
    return undefined
  }

  const cancelS3TransferAsync = async (transferId: string): Promise<void> => {
    await invoke('remote_storage_cancel_transfer', { transferId })
  }

  /**
   * Cancel an in-flight transfer for `file`. Resolves the right transferId
   * from the active-transfer maps (S3: keyed by S3 key, P2P: keyed by
   * remote path) and forwards to the matching backend cancel command.
   * No-op when nothing is in flight for that row — safe to call from the
   * X-button without separately checking.
   */
  const cancelFileTransferAsync = async (file: FileEntry): Promise<void> => {
    const peer = selectedPeer.value
    if (!peer) return
    if (peer.s3BackendId) {
      const key = toS3Prefix(currentPath.value) + file.name
      const transferId = getS3TransferIdForKey(key)
      if (transferId) await cancelS3TransferAsync(transferId)
      return
    }
    if (!peer.localPath) {
      const path = resolveFilePath(file)
      const transferId = peerStore.getTransferIdForPath(path)
      if (transferId) await peerStore.cancelTransferAsync(transferId)
    }
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

  /**
   * Mirror of `startS3ChunkedDownload` for the upload direction. Uses the
   * same `transferIdToKey` + `s3TransferProgress` maps so the per-row UI
   * (progress overlay + X cancel button) lights up exactly the same way it
   * does for downloads — no new plumbing on the consumer side.
   */
  const startS3ChunkedUpload = async (
    backendId: string,
    key: string,
    sourcePath: string,
  ): Promise<void> => {
    const transferId = crypto.randomUUID()
    transferIdToKey.set(transferId, key)
    const next = new Map(s3TransferProgress.value)
    next.set(key, 0)
    s3TransferProgress.value = next

    try {
      await invoke('remote_storage_upload_from_path', {
        request: { backendId, key, sourcePath, transferId },
      })
    } finally {
      transferIdToKey.delete(transferId)
      const cleared = new Map(s3TransferProgress.value)
      cleared.delete(key)
      s3TransferProgress.value = cleared
    }
  }

  return {
    getS3TransferProgress,
    cancelFileTransferAsync,
    startS3ChunkedDownload,
    startS3ChunkedUpload,
  }
}
