import { computed, ref } from 'vue'
import { defineStore } from 'pinia'
import type { FilesError } from '@bindings/FilesError'
import type { SourceRef } from '@bindings/SourceRef'
import type { TransferAnswer } from '@bindings/TransferAnswer'
import type { TransferEvent } from '@bindings/TransferEvent'
import type { TransferOp } from '@bindings/TransferOp'
import type { TransferProgress } from '@bindings/TransferProgress'
import type { TransferTarget } from '@bindings/TransferTarget'
import type { FilesClipboard } from '~/lib/files/clipboard'
import { asFilesError } from '~/composables/useFiles'

/** A transfer as the transfer bar shows it (FR-019). */
export type TransferView = {
  /** Known before the command answers: its events may come first (they are bound to this view). */
  key: number
  id: string | null
  op: TransferOp
  /** The tab that started it; its file browser asks about conflicts. */
  tabId: string
  progress: TransferProgress | null
  /** A name at the target the transfer waits on. */
  conflict: string | null
  /** How many files leave an encrypted folder for an ordinary one; the transfer waits for the
   * answer before it starts (spec 048 FR-019). */
  leaving: number | null
  state: 'running' | 'failed'
  error: FilesError | null
}

/**
 * holzi's clipboard of files and the running transfers (spec 044 FR-018, FR-019, T050). All file
 * browser tabs share them (one webview); a transfer runs on in Rust when its tab closes, and the
 * vault's close ends it (FR-026). Neither is restored with the session.
 */
export const useFilesTransfersStore = defineStore('filesTransfers', () => {
  const clipboard = ref<FilesClipboard | null>(null)
  const transfers = ref<TransferView[]>([])
  const running = computed(() => transfers.value.length > 0)
  let nextKey = 0
  /** Resolves a view's id once the command answered. */
  const ids = new Map<number, Promise<string | null>>()

  const files = useFiles()

  function view(key: number) {
    return transfers.value.find((transfer) => transfer.key === key)
  }

  function drop(key: number) {
    transfers.value = transfers.value.filter((transfer) => transfer.key !== key)
    ids.delete(key)
  }

  function onEvent(key: number, event: TransferEvent, onDone?: () => void) {
    const transfer = view(key)
    if (!transfer) return
    switch (event.kind) {
      case 'progress':
        transfer.progress = event.progress
        break
      case 'conflict':
        if ('leavesEncryption' in event)
          transfer.leaving = event.leavesEncryption.count
        else transfer.conflict = event.name
        break
      case 'done':
        drop(key)
        onDone?.()
        break
      case 'cancelled':
        drop(key)
        break
      case 'failed':
        transfer.state = 'failed'
        transfer.conflict = null
        transfer.leaving = null
        transfer.error = event.error
        break
    }
  }

  /** Shows a transfer from its start on; `begin` invokes the command with the view's events. */
  async function track(
    op: TransferOp,
    tabId: string,
    begin: (onEvent: (event: TransferEvent) => void) => Promise<string>,
    onDone?: () => void,
  ): Promise<void> {
    const key = nextKey++
    transfers.value.push({
      key,
      id: null,
      op,
      tabId,
      progress: null,
      conflict: null,
      leaving: null,
      state: 'running',
      error: null,
    })
    const started = begin((event) => onEvent(key, event, onDone))
    ids.set(
      key,
      started.catch(() => null),
    )
    try {
      const id = await started
      const transfer = view(key)
      if (transfer) transfer.id = id
    } catch (error) {
      drop(key)
      throw error
    }
  }

  /**
   * Starts a transfer; refusals before the start (`intoItself`, `noSpace`, …) reject, so the
   * caller shows them. `onDone` runs once it finished.
   */
  function startAsync(
    op: TransferOp,
    from: SourceRef,
    paths: string[],
    to: TransferTarget | null,
    tabId: string,
    onDone?: () => void,
  ): Promise<void> {
    return track(
      op,
      tabId,
      (onEvent) => files.transferStartAsync(op, from, paths, to, onEvent),
      onDone,
    )
  }

  /** Copies paths dropped from the system into a folder (FR-024). */
  function importAsync(
    paths: string[],
    to: TransferTarget,
    tabId: string,
  ): Promise<void> {
    return track('copy', tabId, (onEvent) =>
      files.importDroppedAsync(paths, to, onEvent),
    )
  }

  async function answerAsync(
    key: number,
    choice: TransferAnswer,
    forAll: boolean,
  ): Promise<void> {
    const transfer = view(key)
    if (transfer) {
      transfer.conflict = null
      transfer.leaving = null
    }
    const id = await ids.get(key)
    if (id) await files.transferAnswerAsync(id, choice, forAll)
  }

  /** Cancels a running transfer, or dismisses a failed one. */
  async function cancelAsync(key: number): Promise<void> {
    const id = await ids.get(key)
    drop(key)
    if (id) await files.transferCancelAsync(id)
  }

  async function retryAsync(key: number): Promise<void> {
    const transfer = view(key)
    const id = await ids.get(key)
    if (!transfer || !id) return
    transfer.state = 'running'
    transfer.error = null
    transfer.progress = null
    try {
      await files.transferRetryAsync(id)
    } catch (error) {
      // Not started again: the bar shows why, and the transfer can only be dismissed.
      transfer.state = 'failed'
      transfer.error = asFilesError(error) ?? {
        code: 'unsupported',
        message: String(error),
      }
    }
  }

  return {
    clipboard,
    transfers,
    running,
    startAsync,
    importAsync,
    answerAsync,
    cancelAsync,
    retryAsync,
  }
})
