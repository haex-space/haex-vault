import { ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { defineStore } from 'pinia'
import { onVaultTablesChanged } from '~/composables/useVaultData'
import { AGENT_MEMORY_TABLES } from '~/lib/agentMemory/pages'
import { useWindowManagerStore } from '~/stores/windowManager'
import type { AgentMemoryList } from '~/types/bindings/AgentMemoryList'
import type { AgentMemoryPage } from '~/types/bindings/AgentMemoryPage'

/**
 * The pages of the agents' shared memory for "KI → Gedächtnis" (spec 097): the list, or the pages
 * matching a search, and the writes of the user. The list follows every change of the table, made
 * here, by an agent or by a sync, through `vault-data-changed`. Writes run the guardrail actions
 * `settings.agentMemory.*`, which no agent can call, and throw what the command refused with.
 *
 * State across remounts (the store outlives the view): `list` and `query` are what the view showed
 * last; the view resets both when it mounts by calling {@link searchAsync} with its own field.
 */
export const useAgentMemoryStore = defineStore('agentMemory', () => {
  const list = ref<AgentMemoryList | null>(null)
  const query = ref('')
  const wm = useWindowManagerStore()
  let searchRevision = 0
  let pendingRefresh = false

  /** Loads the list for `next` (empty: every page); throws what the command throws. */
  async function searchAsync(next: string): Promise<void> {
    query.value = next
    const revision = ++searchRevision
    const loaded = await invoke<AgentMemoryList>('agent_memory_list', {
      args: { query: next.trim() || null },
    })
    // Ignore an earlier request even when the query text is the same: a later refresh may have
    // loaded newer pages after a write or sync.
    if (revision === searchRevision) {
      list.value = loaded
      if (pendingRefresh) {
        pendingRefresh = false
        void searchAsync(query.value).catch((error) => {
          console.error('[agent-memory] reloading after a change failed', error)
        })
      }
    }
  }

  /** The page `id` with its pointers; throws `InvalidInput` `not_found` when it is gone. */
  function pageAsync(id: string): Promise<AgentMemoryPage> {
    return invoke<AgentMemoryPage>('agent_memory_page', { args: { id } })
  }

  async function runUserAction(
    id: string,
    input: Record<string, unknown>,
  ): Promise<unknown> {
    const outcome = await wm.runAction(id, input, { kind: 'user' })
    if (outcome.ok) return outcome.result
    throw outcome.error !== undefined
      ? outcome.error
      : new Error(outcome.message)
  }

  async function addAsync(page: {
    title: string
    description: string
    body: string
  }): Promise<AgentMemoryPage> {
    return (await runUserAction(
      'settings.agentMemory.add',
      page,
    )) as AgentMemoryPage
  }

  async function changeAsync(page: {
    id: string
    title: string
    description: string
    body: string
  }): Promise<AgentMemoryPage> {
    return (await runUserAction(
      'settings.agentMemory.change',
      page,
    )) as AgentMemoryPage
  }

  async function removeAsync(id: string): Promise<void> {
    await runUserAction('settings.agentMemory.remove', { id })
  }

  onVaultTablesChanged(AGENT_MEMORY_TABLES, async () => {
    if (list.value === null) {
      pendingRefresh = true
      return
    }
    try {
      await searchAsync(query.value)
    } catch (error) {
      console.error('[agent-memory] reloading after a change failed', error)
    }
  })

  return {
    list,
    query,
    searchAsync,
    pageAsync,
    addAsync,
    changeAsync,
    removeAsync,
  }
})
