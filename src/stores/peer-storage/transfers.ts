import { invoke, Channel } from '@tauri-apps/api/core'
import type { Ref } from 'vue'
import type { DirEntry } from '~/../src-tauri/bindings/DirEntry'

export interface TransferProgress {
  transferId: string
  path: string
  fileName: string
  direction: 'download' | 'upload'
  bytesReceived: number
  totalBytes: number
  progress: number // 0-1
  startedAt: number // Date.now() at first progress tick
  bytesPerSec: number // EMA-smoothed throughput, alpha = 0.3
  paused: boolean
}

export interface TransfersContext {
  transfers: Ref<Map<string, TransferProgress>>
  activeTransfers: Ref<number>
}

type TransferEvent =
  | { event: 'progress'; bytesReceived: number; totalBytes: number }
  | { event: 'complete'; localPath: string; totalBytes: number }
  | { event: 'error'; error: string }

export type CreateTransferChannel = (
  transferId: string,
  path: string,
  direction: 'download' | 'upload',
) => { channel: Channel<TransferEvent>; promise: Promise<string> }

export const createTransfersModule = (ctx: TransfersContext) => {
  const { transfers, activeTransfers } = ctx

  const createTransferChannel: CreateTransferChannel = (
    transferId,
    path,
    direction,
  ) => {

    let resolveTransfer: ((localPath: string) => void) | undefined
    let rejectTransfer: ((error: Error) => void) | undefined
    const fileName = path.split('/').pop() || path

    const startedAt = Date.now()
    let lastSampleAt = startedAt
    let lastBytes = 0
    // EMA so the displayed rate doesn't twitch on every 100 ms progress tick.
    // Seeded from the first instant rate so the chip is not stuck on 0 B/s
    // for the first ~1 s of a transfer.
    let smoothedBytesPerSec = 0

    const promise = new Promise<string>((resolve, reject) => {
      resolveTransfer = resolve
      rejectTransfer = reject
    })

    const channel = new Channel<TransferEvent>()
    channel.onmessage = (msg) => {
      switch (msg.event) {
        case 'progress': {
          // Snapshot the paused flag from the prior tick. A trailing chunk can
          // still arrive after pause (the backend cancels at the next chunk
          // boundary, not mid-chunk); skipping the EMA update there keeps the
          // displayed rate honest at 0 B/s for paused transfers.
          const paused = transfers.value.get(transferId)?.paused ?? false
          const now = Date.now()
          const dt = (now - lastSampleAt) / 1000
          if (!paused && dt > 0) {
            const instant = (msg.bytesReceived - lastBytes) / dt
            smoothedBytesPerSec
              = smoothedBytesPerSec === 0 ? instant : 0.3 * instant + 0.7 * smoothedBytesPerSec
          }
          lastSampleAt = now
          lastBytes = msg.bytesReceived

          transfers.value.set(transferId, {
            transferId,
            path,
            fileName,
            direction,
            bytesReceived: msg.bytesReceived,
            totalBytes: msg.totalBytes,
            progress: msg.totalBytes > 0 ? msg.bytesReceived / msg.totalBytes : 0,
            startedAt,
            bytesPerSec: paused ? 0 : smoothedBytesPerSec,
            paused,
          })
          transfers.value = new Map(transfers.value)
          break
        }
        case 'complete': {
          const transfer = transfers.value.get(transferId)
          if (transfer) {
            transfer.progress = 1
            // Zero the rate so the aggregate `totalBytesPerSec` chip doesn't
            // keep the just-completed transfer's last sample alive during the
            // 1.5 s linger window — otherwise the chip lies about being busy.
            transfer.bytesPerSec = 0
            transfers.value = new Map(transfers.value)
            setTimeout(() => {
              transfers.value.delete(transferId)
              transfers.value = new Map(transfers.value)
            }, 1500)
          }
          resolveTransfer?.(msg.localPath)
          break
        }
        case 'error':
          transfers.value.delete(transferId)
          transfers.value = new Map(transfers.value)
          rejectTransfer?.(new Error(msg.error))
          break
      }
    }

    return { channel, promise }
  }

  const getTransferProgress = (filePath: string): number | undefined => {
    for (const t of transfers.value.values()) {
      if (t.path === filePath) return t.progress
    }
    return undefined
  }

  const getTransferIdForPath = (filePath: string): string | undefined => {
    for (const t of transfers.value.values()) {
      if (t.path === filePath) return t.transferId
    }
    return undefined
  }

  const cancelTransferAsync = async (transferId: string) => {
    await invoke('peer_storage_transfer_cancel', { transferId })
    transfers.value.delete(transferId)
    transfers.value = new Map(transfers.value)
  }

  const setTransferPaused = (transferId: string, paused: boolean) => {
    const t = transfers.value.get(transferId)
    if (t) {
      t.paused = paused
      // Zero the throughput chip while paused so it doesn't keep showing the
      // pre-pause rate; it recovers from the next progress tick after resume.
      if (paused) t.bytesPerSec = 0
      transfers.value = new Map(transfers.value)
    }
  }

  const pauseTransferAsync = async (transferId: string) => {
    await invoke('peer_storage_transfer_pause', { transferId })
    setTransferPaused(transferId, true)
  }

  const resumeTransferAsync = async (transferId: string) => {
    await invoke('peer_storage_transfer_resume', { transferId })
    setTransferPaused(transferId, false)
  }

  const getTransferPaused = (filePath: string): boolean => {
    for (const t of transfers.value.values()) {
      if (t.path === filePath) return t.paused
    }
    return false
  }

  return {
    createTransferChannel,
    getTransferProgress,
    getTransferIdForPath,
    cancelTransferAsync,
    pauseTransferAsync,
    resumeTransferAsync,
    getTransferPaused,
    activeTransfers,
  }
}

// =========================================================================
// Local filesystem helpers (pure)
// =========================================================================

const isContentUri = (p: string) => p.startsWith('{')

const resolveLocalPath = (localPath: string, subPath: string) => {
  if (subPath === '/' || !subPath) return localPath
  if (isContentUri(subPath)) return subPath
  return `${localPath}/${subPath.replace(/^\//, '')}`
}

const mapDirEntry = (e: DirEntry) => ({
  name: e.name,
  path: e.path,
  size: BigInt(e.size),
  isDir: e.isDirectory,
  modified: e.modified ? BigInt(e.modified) / 1000n : null,
})

export const localListAsync = async (
  localPath: string,
  subPath: string,
  offset?: number,
  limit?: number,
) => {
  const target = resolveLocalPath(localPath, subPath)
  const result = await invoke<{ entries: DirEntry[]; total: number }>('filesystem_read_dir', {
    path: target,
    offset: offset ?? null,
    limit: limit ?? null,
  })
  return { entries: result.entries.map(mapDirEntry), total: result.total }
}
