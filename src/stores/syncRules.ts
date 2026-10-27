import { defineStore } from 'pinia'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import type { SyncRuleView } from '@bindings/SyncRuleView'

const SYNC_RULES_CHANGED = 'sync-rules-changed'

/**
 * The sync rules of this device (spec 061 US1, FR-025, FR-032): the list with each rule's status.
 * One listener for `sync-rules-changed` reloads it, so a run starting or ending shows without
 * reloading; events during a load fold into one more load.
 *
 * Pinia keeps this store for the session. `rules` and `loading` are the backend's state and survive
 * a remount on purpose; `error` belongs to the open overview and is reset by `initialize`.
 */
export const useSyncRulesStore = defineStore('syncRules', () => {
  const { listAsync } = useSyncRules()
  const { errString } = useErrorString()

  const rules = ref<SyncRuleView[]>([])
  const loading = ref(true)
  const error = ref<string | null>(null)

  let unlisten: UnlistenFn | null = null
  let pendingListen: Promise<void> | null = null
  let listenGeneration = 0
  let loadingNow = false
  let again = false

  function initialize(): void {
    error.value = null
  }

  /** Loads the list; a call during a load asks for one more. */
  async function loadAsync(): Promise<void> {
    if (loadingNow) {
      again = true
      return
    }
    loadingNow = true
    try {
      do {
        again = false
        try {
          rules.value = await listAsync()
          error.value = null
        } catch (e) {
          error.value = errString(e)
        }
      } while (again)
    } finally {
      loadingNow = false
      loading.value = false
    }
  }

  async function startListening(): Promise<void> {
    if (unlisten) return
    if (pendingListen) return await pendingListen

    const generation = ++listenGeneration
    const registration = listen(SYNC_RULES_CHANGED, () => {
      void loadAsync()
    }).then((cleanup) => {
      if (
        generation !== listenGeneration ||
        pendingListen !== registration ||
        unlisten
      ) {
        cleanup()
        return
      }
      unlisten = cleanup
    })
    pendingListen = registration
    try {
      await registration
    } finally {
      if (pendingListen === registration) pendingListen = null
    }
  }

  function stopListening(): void {
    listenGeneration += 1
    unlisten?.()
    unlisten = null
    pendingListen = null
  }

  return {
    rules,
    loading,
    error,
    initialize,
    loadAsync,
    startListening,
    stopListening,
  }
})
