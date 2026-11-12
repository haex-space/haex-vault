import { computed, ref, shallowRef } from 'vue'
import { defineStore } from 'pinia'
import type { GroupRow } from '@bindings/GroupRow'
import type { ItemHeader } from '@bindings/ItemHeader'
import type { TagRow } from '@bindings/TagRow'
import { inScope, loginTagsOf } from '~/lib/passwords/scope'
import {
  keepUnchanged,
  mergeTagsForDisplay,
  tagIdsFor,
} from '~/lib/passwords/remote'

/**
 * The overview of the password manager (spec 034, research R15): every entry header, folder and
 * tag the window shows, loaded by one command and reloaded quietly when a table changes — through
 * another window, an agent, an extension or a sync. The store holds headers only, never a secret
 * (FR-040); a secret is fetched for the moment the user asks and kept in a component-local ref.
 *
 * A quiet reload replaces the lists but never touches a draft: drafts are local to the editor. The
 * lists are shallow and keep the object of every unchanged row, so a reload re-renders only the
 * rows that changed.
 */
export const usePasswordsStore = defineStore('passwords', () => {
  const { loadOverviewAsync } = usePasswords()
  const { errString } = useErrorString()

  const headers = shallowRef<readonly ItemHeader[]>([])
  const groups = shallowRef<readonly GroupRow[]>([])
  const tags = shallowRef<readonly TagRow[]>([])
  const isLoading = ref(false)
  const hasLoadedOnce = ref(false)
  const lastError = ref<string | null>(null)

  /** Entries by id, for the places that hold only an id (every place carries ids only). */
  const headersById = computed(
    () => new Map(headers.value.map((header) => [header.id, header])),
  )

  /** The tags as the window shows them: equal names (two devices made the tag) show as one until
   * the cleanup at the next open merges them for good. */
  const displayTags = computed(() =>
    mergeTagsForDisplay(tags.value, headers.value),
  )

  /**
   * The entries the password manager shows: everything but the cards and tickets, which are
   * Wallet's (spec 092, Phase W). The store keeps every header, so both apps share one overview and
   * one reload; the list, the folder tree and its counts, the search, the tag filter, the trash and
   * the pickers of the password manager read this view.
   */
  const loginHeaders = computed(() => inScope(headers.value, 'login'))

  /** The tags of the password manager: counted over its entries only, and a tag that only cards or
   * tickets carry is left out. */
  const loginTags = computed(() =>
    loginTagsOf(displayTags.value, headers.value),
  )

  /** The ids a filter by `tagId` has to accept. */
  function tagIdsOf(tagId: string): string[] {
    return tagIdsFor(tagId, displayTags.value)
  }

  async function loadAsync() {
    const overview = await loadOverviewAsync()
    headers.value = keepUnchanged(headers.value, overview.headers)
    groups.value = keepUnchanged(groups.value, overview.groups)
    tags.value = keepUnchanged(tags.value, overview.tags)
    lastError.value = null
    hasLoadedOnce.value = true
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

  /** The reload a change event triggers: no spinner, no flash, an error is kept for the banner. */
  async function quietReloadAsync() {
    try {
      await loadAsync()
    } catch (error) {
      lastError.value = errString(error)
    }
  }

  onVaultTablesChanged(['haex_passwords_*'], quietReloadAsync)

  return {
    headers,
    loginHeaders,
    loginTags,
    groups,
    tags,
    displayTags,
    tagIdsOf,
    headersById,
    isLoading,
    hasLoadedOnce,
    lastError,
    reloadAsync,
    quietReloadAsync,
  }
})
