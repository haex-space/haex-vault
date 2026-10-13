/**
 * Storage dialogs of extensions (spec 038, US3, research R6), in two steps. An extension's
 * request (`extension-storage-request`) first shows over its own tab and asks only for a
 * confirmation; that dialog never has fields for credentials, because an extension could draw a
 * look-alike inside its frame. When the confirmation needs new credentials, holzi's own window over
 * the whole app asks for them (`StorageCredentialsModal.vue`, mounted in the desktop). Both answer
 * with `storage_dialog_resolve`; the credentials go from that window to Rust only. That window
 * stays open while holzi tests them: a failed test comes back to it (`StorageTrial`), the user
 * corrects the credentials or cancels, and the extension learns only the end.
 */
import { computed, onBeforeUnmount, readonly, ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import type { CredentialsInput } from '@bindings/CredentialsInput'
import type { StorageAnswer } from '@bindings/StorageAnswer'
import type { StorageTrial } from '@bindings/StorageTrial'

export type StorageRequestKind = 'add' | 'update' | 'test' | 'remove'

/** What holzi shows of an extension's proposal; never credentials. `bucket` is the folder of a
 * WebDAV or SFTP storage (spec 050). */
export type StorageProposal = {
  name?: string
  bucket?: string
  protocol?: 's3' | 'webdav' | 'sftp'
  currentName?: string
  currentBucket?: string
  providerName?: string
  endpoint?: string
  region?: string
  insecure?: boolean
  scope?: 'public' | 'local'
  sameProvider?: boolean
  connections?: { id: string; providerName: string }[]
}

export type StorageRequest = {
  requestId: string
  frame: string
  kind: StorageRequestKind
  proposal: StorageProposal
  extensionName: string
  otherExtensions: string[]
}

/** The request whose credentials holzi's window over the whole app asks for. */
const credentialsRequest = ref<StorageRequest | null>(null)
/** Whether the user said in the dialog over the tab to make a missing folder (spec 050 FR-006). */
const credentialsCreateFolder = ref(false)

/** Whether a storage of `protocol` keeps its files in a folder of a server (WebDAV, SFTP). */
export function isFolderProtocol(protocol: StorageProposal['protocol']) {
  return protocol === 'webdav' || protocol === 'sftp'
}

export function resolveStorageAsync(
  requestId: string,
  answer: StorageAnswer,
): Promise<StorageTrial> {
  return invoke<StorageTrial>('storage_dialog_resolve', { requestId, answer })
}

/** The window over the whole app: the request it asks credentials for, and its answers. */
export function useStorageCredentials() {
  return {
    request: readonly(credentialsRequest),
    /**
     * Sends the credentials and waits for their test; `createFolder` makes a missing folder of a
     * WebDAV or SFTP storage, `hostKey` is the SFTP host key the user confirmed. The window closes when the request ends
     * (`extension-storage-request-ended`); a failed test leaves it open.
     */
    async confirmAsync(
      credentials: CredentialsInput,
      createFolder = false,
      hostKey?: string,
    ): Promise<StorageTrial> {
      const open = credentialsRequest.value
      if (!open) return { kind: 'ended' }
      const trial = await resolveStorageAsync(open.requestId, {
        kind: 'confirm',
        credentials,
        createFolder: createFolder || credentialsCreateFolder.value,
        hostKey,
      }).catch((): StorageTrial => ({ kind: 'ended' }))
      if (trial.kind === 'ended' && credentialsRequest.value === open)
        credentialsRequest.value = null
      return trial
    },
    cancel(): void {
      const open = credentialsRequest.value
      if (!open) return
      credentialsRequest.value = null
      void resolveStorageAsync(open.requestId, { kind: 'cancel' }).catch(
        () => {},
      )
    },
  }
}

/** What the user chose in the dialog over the tab. */
export type StorageChoice =
  | { confirm: false }
  | {
      confirm: true
      connectionId?: string
      newCredentials: boolean
      createFolder?: boolean
    }

/**
 * The storage dialog of one extension frame: shows the frame's request, answers it or hands it on
 * to holzi's window for credentials, and ends both when the request ends or the frame closes.
 */
export function useFrameStorageDialog(
  frameOf: () => string | null,
  afterAnswer: () => void,
) {
  const request = ref<StorageRequest | null>(null)
  const unlisten: UnlistenFn[] = []
  let unmounted = false

  function end(requestId: string) {
    if (request.value?.requestId === requestId) request.value = null
    if (credentialsRequest.value?.requestId === requestId)
      credentialsRequest.value = null
  }

  void Promise.all([
    listen<StorageRequest>('extension-storage-request', (event) => {
      if (event.payload.frame === frameOf()) request.value = event.payload
    }),
    listen<{ requestId: string }>('extension-storage-request-ended', (event) =>
      end(event.payload.requestId),
    ),
  ]).then((offs) => {
    if (unmounted) for (const off of offs) off()
    else unlisten.push(...offs)
  })

  onBeforeUnmount(() => {
    unmounted = true
    for (const off of unlisten) off()
    // Holzi cancels the dialogs of a closed frame; its window for credentials goes with them.
    if (credentialsRequest.value?.frame === frameOf())
      credentialsRequest.value = null
  })

  /**
   * holzi's window asks for this frame's credentials: the frame stays inert, so neither a focus
   * call nor the extension's script takes the keyboard away from that window.
   */
  const credentialsPending = computed(
    () =>
      credentialsRequest.value !== null &&
      credentialsRequest.value.frame === frameOf(),
  )

  function answer(choice: StorageChoice): void {
    const open = request.value
    request.value = null
    if (!open) return
    if (choice.confirm && choice.newCredentials && !choice.connectionId) {
      // No `afterAnswer`: focusing the frame now would send the typed credentials to it.
      credentialsCreateFolder.value = choice.createFolder ?? false
      credentialsRequest.value = open
      return
    }
    const answer: StorageAnswer = choice.confirm
      ? {
          kind: 'confirm',
          connectionId: choice.connectionId,
          createFolder: choice.createFolder,
        }
      : { kind: 'cancel' }
    void resolveStorageAsync(open.requestId, answer).catch(() => {})
    afterAnswer()
  }

  return {
    storageRequest: request,
    storageCredentialsPending: credentialsPending,
    answerStorage: answer,
  }
}
