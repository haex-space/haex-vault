/**
 * The file browser's commands (spec 044, contracts/tauri-commands.md). Every call acts as the
 * user; paths are checked in Rust. Errors come back as `FilesError` (`{ code, message }`).
 */
import { Channel, invoke } from '@tauri-apps/api/core'
import type { Entry } from '@bindings/Entry'
import type { FilesError } from '@bindings/FilesError'
import type { FilesPeerRoots } from '@bindings/FilesPeerRoots'
import type { FilesSearchEvent } from '@bindings/FilesSearchEvent'
import type { FolderChanged } from '@bindings/FolderChanged'
import type { Opened } from '@bindings/Opened'
import type { SearchFilters } from '@bindings/SearchFilters'
import type { SourceRef } from '@bindings/SourceRef'
import type { StorageSpace } from '@bindings/StorageSpace'
import type { Sources } from '@bindings/Sources'
import type { TextContent } from '@bindings/TextContent'
import type { TransferAnswer } from '@bindings/TransferAnswer'
import type { TransferEvent } from '@bindings/TransferEvent'
import type { TransferOp } from '@bindings/TransferOp'
import type { TransferTarget } from '@bindings/TransferTarget'

/** A `FilesError` from a rejected command, or `undefined` for anything else. */
export function asFilesError(error: unknown): FilesError | undefined {
  if (
    error &&
    typeof error === 'object' &&
    'code' in error &&
    'message' in error
  ) {
    return error as FilesError
  }
  return undefined
}

export function useFiles() {
  async function sourcesAsync(): Promise<Sources> {
    return await invoke<Sources>('files_sources')
  }

  /** What the storage holds and may hold, `null` where its provider cannot tell (S3). */
  async function storageSpaceAsync(
    storageId: string,
  ): Promise<StorageSpace | null> {
    return await invoke<StorageSpace | null>('storage_space', { storageId })
  }

  /** Whether the files of this device are reachable (FR-039: on Android "Zugriff auf alle Dateien"). */
  async function allAccessAsync(): Promise<boolean> {
    return await invoke<boolean>('files_all_access')
  }

  /** Leads to the system's setting for it; on Android before version 11 asks right away. */
  async function requestAllAccessAsync(): Promise<void> {
    await invoke('files_request_all_access')
  }

  async function listAsync(source: SourceRef, path: string): Promise<Entry[]> {
    return await invoke<Entry[]>('files_list', { source, path })
  }

  async function statAsync(source: SourceRef, path: string): Promise<Entry> {
    return await invoke<Entry>('files_stat', { source, path })
  }

  async function readTextAsync(
    source: SourceRef,
    path: string,
  ): Promise<TextContent> {
    return await invoke<TextContent>('files_read_text', { source, path })
  }

  /** Opens a file for the viewer of `tabId`; images, video, audio and PDF get a media server URL
   * that lives until it is released or the tab goes away. */
  async function openAsync(
    source: SourceRef,
    path: string,
    tabId: string,
  ): Promise<Opened> {
    return await invoke<Opened>('files_open', { source, path, tabId })
  }

  async function releaseAsync(url: string): Promise<void> {
    await invoke('files_release', { url })
  }

  async function releaseTabAsync(tabId: string): Promise<void> {
    await invoke('files_release_tab', { tabId })
  }

  async function thumbnailAsync(
    source: SourceRef,
    entry: Entry,
  ): Promise<ArrayBuffer> {
    return await invoke<ArrayBuffer>('files_thumbnail', {
      source,
      path: entry.path,
      size: entry.size ?? 0,
      modifiedMs: entry.modifiedMs ?? 0,
    })
  }

  /** Watches `path` of this device, or of the other own device `storage` (spec 056); `onChange`
   * runs for every debounced batch, and once more when a watch on another device ends. Resolves
   * to the watch id. */
  async function watchAsync(
    path: string,
    onChange: (change: FolderChanged) => void,
    storage: string | null = null,
  ): Promise<number> {
    const channel = new Channel<FolderChanged>()
    channel.onmessage = onChange
    return await invoke<number>('files_watch', { path, storage, channel })
  }

  /** The drives and known places of the other own device `storageId` (spec 056). */
  async function peerRootsAsync(storageId: string): Promise<FilesPeerRoots> {
    return await invoke<FilesPeerRoots>('files_peer_roots', { storageId })
  }

  /** Whether deleting on the other device or share `storageId` goes to its trash (spec 056). */
  async function peerTrashAsync(storageId: string): Promise<boolean> {
    return await invoke<boolean>('files_peer_trash', { storageId })
  }

  async function unwatchAsync(id: number): Promise<void> {
    await invoke('files_unwatch', { id })
  }

  /** Opens a file with the system's app. A file of an encrypted folder answers `needsNotice`
   * until the window showed its notice and calls again with `confirmed` (spec 048 FR-028). */
  async function openSystemAsync(
    source: SourceRef,
    path: string,
    confirmed = false,
    dontAskAgain = false,
  ): Promise<void> {
    await invoke('files_open_system', {
      source,
      path,
      confirmed,
      dontAskAgain,
    })
  }

  async function createFolderAsync(
    source: SourceRef,
    path: string,
    name: string,
  ): Promise<Entry> {
    return await invoke<Entry>('files_create_folder', { source, path, name })
  }

  async function renameAsync(
    source: SourceRef,
    path: string,
    newName: string,
  ): Promise<Entry> {
    return await invoke<Entry>('files_rename', { source, path, newName })
  }

  /** A new encrypted folder in the storage folder `path` (spec 048 FR-001). */
  async function createEncryptedFolderAsync(
    source: SourceRef,
    path: string,
    name: string,
  ): Promise<Entry> {
    return await invoke<Entry>('files_create_encrypted_folder', {
      source,
      path,
      name,
    })
  }

  /** Whether this device showed the notice of FR-002 already. */
  async function encryptedNoticeShownAsync(): Promise<boolean> {
    const answer = await invoke<{ shown: boolean }>('files_encrypted_notice')
    return answer.shown
  }

  async function encryptedNoticeAckAsync(): Promise<void> {
    await invoke('files_encrypted_notice_ack')
  }

  /** Loads the encrypted folder under `path` again from its storage. */
  async function encryptedRefreshAsync(
    source: SourceRef,
    path: string,
  ): Promise<void> {
    await invoke('files_encrypted_refresh', { source, path })
  }

  /** Starts a copy, move or delete; `onEvent` hears its progress, conflicts and end. Refusals
   * before the start (`intoItself`, `noSpace`, …) reject. Resolves to the transfer id. */
  async function transferStartAsync(
    op: TransferOp,
    from: SourceRef,
    paths: string[],
    to: TransferTarget | null,
    onEvent: (event: TransferEvent) => void,
  ): Promise<string> {
    const channel = new Channel<TransferEvent>()
    channel.onmessage = onEvent
    return await invoke<string>('files_transfer_start', {
      op,
      from,
      paths,
      to,
      channel,
    })
  }

  /** Copies paths dropped from the system into a folder (FR-024). */
  async function importDroppedAsync(
    paths: string[],
    to: TransferTarget,
    onEvent: (event: TransferEvent) => void,
  ): Promise<string> {
    const channel = new Channel<TransferEvent>()
    channel.onmessage = onEvent
    return await invoke<string>('files_import_dropped', { paths, to, channel })
  }

  async function transferAnswerAsync(
    transferId: string,
    choice: TransferAnswer,
    forAll: boolean,
  ): Promise<void> {
    await invoke('files_transfer_answer', { transferId, choice, forAll })
  }

  async function transferCancelAsync(transferId: string): Promise<void> {
    await invoke('files_transfer_cancel', { transferId })
  }

  async function transferPauseAsync(
    transferId: string,
    paused: boolean,
  ): Promise<void> {
    await invoke('files_transfer_pause', { transferId, paused })
  }

  async function transferRetryAsync(transferId: string): Promise<void> {
    await invoke('files_transfer_retry', { transferId })
  }

  /** Searches `path` and below; `onEvent` hears hits in batches and the end. Resolves to the
   * search id for `searchCancelAsync`. */
  async function searchStartAsync(
    source: SourceRef,
    path: string,
    query: string,
    filters: SearchFilters,
    showHidden: boolean,
    onEvent: (event: FilesSearchEvent) => void,
  ): Promise<string> {
    const channel = new Channel<FilesSearchEvent>()
    channel.onmessage = onEvent
    return await invoke<string>('files_search_start', {
      source,
      path,
      query,
      filters,
      showHidden,
      channel,
    })
  }

  async function searchCancelAsync(searchId: string): Promise<void> {
    await invoke('files_search_cancel', { searchId })
  }

  return {
    sourcesAsync,
    storageSpaceAsync,
    allAccessAsync,
    requestAllAccessAsync,
    listAsync,
    statAsync,
    readTextAsync,
    openAsync,
    releaseAsync,
    releaseTabAsync,
    thumbnailAsync,
    watchAsync,
    unwatchAsync,
    peerRootsAsync,
    peerTrashAsync,
    openSystemAsync,
    createFolderAsync,
    renameAsync,
    createEncryptedFolderAsync,
    encryptedNoticeShownAsync,
    encryptedNoticeAckAsync,
    encryptedRefreshAsync,
    transferStartAsync,
    importDroppedAsync,
    transferAnswerAsync,
    transferCancelAsync,
    transferPauseAsync,
    transferRetryAsync,
    searchStartAsync,
    searchCancelAsync,
  }
}
