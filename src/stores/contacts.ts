import { computed, ref, shallowRef } from 'vue'
import { defineStore } from 'pinia'
import type { ContactGroup } from '@bindings/ContactGroup'
import type { ContactHeader } from '@bindings/ContactHeader'
import type { ContactsPhoneSummary } from '@bindings/ContactsPhoneSummary'
import { keepUnchanged } from '~/lib/passwords/remote'

/**
 * The overview of the address book (spec 051): every contact header and group the window shows,
 * reloaded quietly when a table changes — through another window, an agent, a phone or a sync. A
 * quiet reload never touches a draft (drafts live in the editor) and keeps the object of every
 * unchanged row, so only changed rows re-render.
 */
export const useContactsStore = defineStore('contacts', () => {
  const { listAsync, groupsAsync } = useContacts()
  const { phonesAsync } = useContactTargets()
  const { errString } = useErrorString()

  const headers = shallowRef<readonly ContactHeader[]>([])
  const groups = shallowRef<readonly ContactGroup[]>([])
  /** The phones of the vault with the sync on: the targets a contact can get (FR-018c). */
  const phones = shallowRef<readonly ContactsPhoneSummary[]>([])
  const isLoading = ref(false)
  const hasLoadedOnce = ref(false)
  const lastError = ref<string | null>(null)

  const headersById = computed(
    () => new Map(headers.value.map((header) => [header.id, header])),
  )

  async function loadAsync() {
    const [nextHeaders, nextGroups, nextPhones] = await Promise.all([
      listAsync(),
      groupsAsync(),
      phonesAsync(),
    ])
    headers.value = keepUnchanged(headers.value, nextHeaders)
    groups.value = keepUnchanged(groups.value, nextGroups)
    phones.value = nextPhones
    lastError.value = null
    hasLoadedOnce.value = true
  }

  /** The name of a phone of the vault. */
  function phoneName(device: string): string | null {
    return phones.value.find((phone) => phone.device === device)?.name ?? null
  }

  /** The first load and an explicit reload: shows the loading state. */
  async function reloadAsync() {
    isLoading.value = true
    try {
      await loadAsync()
    } catch (error) {
      lastError.value = errString(error)
    } finally {
      isLoading.value = false
    }
  }

  /** The reload a change event triggers: no spinner, an error is kept for the banner. */
  async function quietReloadAsync() {
    try {
      await loadAsync()
    } catch (error) {
      lastError.value = errString(error)
    }
  }

  onVaultTablesChanged(['haex_contacts*'], quietReloadAsync)
  // A phone that switches its sync on or off, or reports its accounts, changes device settings.
  onVaultTablesChanged(['preferences'], async () => {
    try {
      phones.value = await phonesAsync()
    } catch (error) {
      lastError.value = errString(error)
    }
  })

  return {
    headers,
    groups,
    phones,
    phoneName,
    headersById,
    isLoading,
    hasLoadedOnce,
    lastError,
    reloadAsync,
    quietReloadAsync,
  }
})
