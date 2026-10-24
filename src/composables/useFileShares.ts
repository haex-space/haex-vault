/**
 * File shares of this vault from the window (spec 056 US2, contracts/tauri-commands.md): the share
 * of an entry, saving and deleting it, and asking a contact's devices for the shares it gave.
 */
import { invoke } from '@tauri-apps/api/core'
import { toast } from 'vue-sonner'
import type { Entry } from '@bindings/Entry'
import type { FileShare } from '@bindings/FileShare'
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

  /** Asks every device of the contact for its shares (FR-028); only on the person's action. */
  async function contactQueryAsync(
    contactId: string,
  ): Promise<FilesContactShares> {
    return await invoke<FilesContactShares>('files_contact_query', {
      contactId,
    })
  }

  return { shareOfAsync, contactQueryAsync }
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
  const { t } = useI18n()
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
    let answer: FilesContactShares
    try {
      answer = await contactQueryAsync(contactId)
    } catch (cause) {
      console.error('[files] querying contact shares failed', cause)
      toast.error(t('files.shares.queryFailed'))
      return
    }
    const current = sources.state.value
    if (!current) return
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
