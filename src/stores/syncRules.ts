import { defineStore } from 'pinia'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import type { SyncRuleProgress } from '@bindings/SyncRuleProgress'
import type { SyncRuleView } from '@bindings/SyncRuleView'

const SYNC_RULES_CHANGED = 'sync-rules-changed'
const SYNC_RULE_PROGRESS = 'sync-rule-progress'

/**
 * The sync rules of this device (spec 061 US1, FR-025, FR-032): the list with each rule's status.
 * One listener for `sync-rules-changed` reloads it, so a run starting or ending shows without
 * reloading; events during a load fold into one more load. `sync-rule-progress` keeps the latest
 * progress of each running rule; it is dropped with the run, whatever the last report was.
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
  /** The latest progress of each running rule, by rule id. */
  const progress = ref<Record<string, SyncRuleProgress>>({})

  let unlisten: UnlistenFn | null = null
  let pendingListen: Promise<void> | null = null
  let listenGeneration = 0
  let loadingNow = false
  let again = false

  function initialize(): void {
    error.value = null
  }

  function dropFinishedProgress(): void {
    const running = new Set(
      rules.value
        .filter((view) => view.status.state === 'running')
        .map((view) => view.rule.id),
    )
    progress.value = Object.fromEntries(
      Object.entries(progress.value).filter(([id]) => running.has(id)),
    )
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
          dropFinishedProgress()
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
    const registration = Promise.resolve().then(async () => {
      const [changedResult, progressResult] = await Promise.allSettled([
        listen(SYNC_RULES_CHANGED, () => {
          void loadAsync()
        }),
        listen<SyncRuleProgress>(SYNC_RULE_PROGRESS, (event) => {
          progress.value = {
            ...progress.value,
            [event.payload.ruleId]: event.payload,
          }
        }),
      ])
      if (changedResult.status === 'rejected') {
        if (progressResult.status === 'fulfilled') progressResult.value()
        throw changedResult.reason
      }
      if (progressResult.status === 'rejected') {
        changedResult.value()
        throw progressResult.reason
      }
      const stopChanged = changedResult.value
      const stopProgress = progressResult.value
      const cleanup = () => {
        stopChanged()
        stopProgress()
      }
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
    progress,
    initialize,
    loadAsync,
    startListening,
    stopListening,
  }
})
