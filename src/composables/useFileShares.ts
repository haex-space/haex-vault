/**
 * File shares of this vault from the window (spec 056 US2, contracts/tauri-commands.md): the share
 * of an entry, saving and deleting it, and asking a contact's devices for the shares it gave.
 */
import { invoke } from '@tauri-apps/api/core'
import type { Entry } from '@bindings/Entry'
import type { FileShare } from '@bindings/FileShare'
import type { FileShareSaveArgs } from '@bindings/FileShareSaveArgs'
import type { FilesContactShares } from '@bindings/FilesContactShares'
import type { SourceRef } from '@bindings/SourceRef'
import type { Sources } from '@bindings/Sources'

export function useFileShares() {
  async function shareOfAsync(
    source: SourceRef,
    path: string,
  ): Promise<FileShare | null> {
    return await invoke<FileShare | null>('file_share_of', { source, path })
  }

  async function saveAsync(args: FileShareSaveArgs): Promise<FileShare> {
    return await invoke<FileShare>('file_share_save', { args })
  }

  async function deleteAsync(shareId: string): Promise<void> {
    await invoke('file_share_delete', { shareId })
  }

  /** Asks every device of the contact for its shares (FR-028); only on the person's action. */
  async function contactQueryAsync(
    contactId: string,
  ): Promise<FilesContactShares> {
    return await invoke<FilesContactShares>('files_contact_query', {
      contactId,
    })
  }

  return { shareOfAsync, saveAsync, deleteAsync, contactQueryAsync }
}

/**
 * The shares of a file browser tab (spec 056 US2): the open source as a share a contact gave this
 * vault (read only at level read, FR-034), the share dialog for an entry, and asking a contact's
 * devices when the person expands it (FR-028), whose answer replaces that contact in `sources`.
 */
export function useFilesTabShares(
  sources: { state: Ref<Sources | null> },
  source: Ref<SourceRef>,
) {
  const { contactQueryAsync } = useFileShares()
  const sharing = ref(false)
  const sharingEntry = ref<Entry | null>(null)

  const receivedShare = computed(() => {
    const open = source.value
    if (open.kind !== 'storage') return undefined
    return sources.state.value?.contacts
      .flatMap((contact) => contact.shares)
      .find((share) => share.storageId === open.storageId)
  })
  const readOnlySource = computed(() => receivedShare.value?.level === 'read')

  function share(entry: Entry) {
    sharingEntry.value = entry
    sharing.value = true
  }

  async function queryContactAsync(contactId: string) {
    const answer = await contactQueryAsync(contactId).catch(() => null)
    const current = sources.state.value
    if (!answer || !current) return
    sources.state.value = {
      ...current,
      contacts: current.contacts.map((contact) =>
        contact.contactId === contactId ? answer : contact,
      ),
    }
  }

  return {
    sharing,
    sharingEntry,
    receivedShare,
    readOnlySource,
    share,
    queryContactAsync,
  }
}
