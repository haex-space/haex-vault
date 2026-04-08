import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { eq } from 'drizzle-orm'
import { haexSyncRules, haexSyncState, type SelectHaexSyncRules } from '~/database/schemas'

interface SyncRuleStatus {
  ruleId: string
  running: boolean
}

interface SyncResult {
  filesDownloaded: number
  filesDeleted: number
  directoriesCreated: number
  bytesTransferred: number
  conflictsResolved: number
  errors: string[]
}

interface SyncProgress {
  currentFile: string
  filesDone: number
  filesTotal: number
  bytesDone: number
  bytesTotal: number
}

export const useFileSyncStore = defineStore('fileSyncStore', () => {
  const { currentVault } = storeToRefs(useVaultStore())

  const syncRules = ref<SelectHaexSyncRules[]>([])
  const syncStatuses = ref<Map<string, SyncRuleStatus>>(new Map())
  const lastResults = ref<Map<string, SyncResult>>(new Map())
  const currentProgress = ref<Map<string, SyncProgress>>(new Map())

  // =========================================================================
  // CRUD operations via Drizzle
  // =========================================================================

  const loadRulesAsync = async () => {
    const db = currentVault.value?.drizzle
    if (!db) return
    syncRules.value = await db.select().from(haexSyncRules).all()
  }

  const createRuleAsync = async (rule: typeof haexSyncRules.$inferInsert) => {
    const db = currentVault.value?.drizzle
    if (!db) throw new Error('No vault open')
    await db.insert(haexSyncRules).values(rule)
    await loadRulesAsync()
  }

  const updateRuleAsync = async (id: string, updates: Partial<typeof haexSyncRules.$inferInsert>) => {
    const db = currentVault.value?.drizzle
    if (!db) throw new Error('No vault open')
    await db.update(haexSyncRules).set(updates).where(eq(haexSyncRules.id, id))
    await loadRulesAsync()
  }

  const deleteRuleAsync = async (id: string) => {
    const db = currentVault.value?.drizzle
    if (!db) throw new Error('No vault open')
    // Stop sync if running
    try { await invoke('file_sync_stop_rule', { ruleId: id }) } catch { /* may not be running */ }
    // Delete sync state
    await db.delete(haexSyncState).where(eq(haexSyncState.ruleId, id))
    // Delete rule
    await db.delete(haexSyncRules).where(eq(haexSyncRules.id, id))
    await loadRulesAsync()
  }

  const toggleRuleAsync = async (id: string, enabled: boolean) => {
    await updateRuleAsync(id, { enabled })
    const rule = syncRules.value.find(r => r.id === id)
    if (!rule) return
    if (enabled) {
      await startRuleAsync(rule)
    } else {
      try { await invoke('file_sync_stop_rule', { ruleId: id }) } catch { /* ok */ }
    }
  }

  // =========================================================================
  // Sync control (delegates to Tauri commands)
  // =========================================================================

  const startRuleAsync = async (rule: SelectHaexSyncRules) => {
    await invoke('file_sync_start_rule', {
      ruleId: rule.id,
      sourceType: rule.sourceType,
      sourceConfig: rule.sourceConfig,
      targetType: rule.targetType,
      targetConfig: rule.targetConfig,
      direction: rule.direction,
      deleteMode: rule.deleteMode,
      intervalSeconds: rule.syncIntervalSeconds,
    })
  }

  const triggerSyncNowAsync = async (ruleId: string) => {
    const rule = syncRules.value.find(r => r.id === ruleId)
    if (!rule) throw new Error('Rule not found')
    const result = await invoke<SyncResult>('file_sync_trigger_now', {
      ruleId: rule.id,
      sourceType: rule.sourceType,
      sourceConfig: rule.sourceConfig,
      targetType: rule.targetType,
      targetConfig: rule.targetConfig,
      direction: rule.direction,
      deleteMode: rule.deleteMode,
    })
    lastResults.value.set(ruleId, result)
    lastResults.value = new Map(lastResults.value) // trigger reactivity
    return result
  }

  const refreshStatusAsync = async () => {
    const statuses = await invoke<SyncRuleStatus[]>('file_sync_status')
    syncStatuses.value = new Map(statuses.map(s => [s.ruleId, s]))
  }

  // Auto-start all enabled rules relevant to this device
  const startEnabledRulesAsync = async () => {
    for (const rule of syncRules.value) {
      if (!rule.enabled) continue
      try {
        await startRuleAsync(rule)
      } catch (e) {
        console.warn(`[FileSync] Failed to start rule ${rule.id}:`, e)
      }
    }
  }

  // =========================================================================
  // Event listeners
  // =========================================================================

  let unlistenProgress: (() => void) | null = null
  let unlistenComplete: (() => void) | null = null
  let unlistenError: (() => void) | null = null

  const setupEventListeners = async () => {
    unlistenProgress = await listen<{ ruleId: string } & SyncProgress>('file-sync:progress', (event) => {
      currentProgress.value.set(event.payload.ruleId, event.payload)
      currentProgress.value = new Map(currentProgress.value)
    })

    unlistenComplete = await listen<{ ruleId: string; result: SyncResult }>('file-sync:complete', (event) => {
      lastResults.value.set(event.payload.ruleId, event.payload.result)
      lastResults.value = new Map(lastResults.value)
      currentProgress.value.delete(event.payload.ruleId)
      currentProgress.value = new Map(currentProgress.value)
    })

    unlistenError = await listen<{ ruleId: string; error: string }>('file-sync:error', (event) => {
      console.error(`[FileSync] Rule ${event.payload.ruleId} error:`, event.payload.error)
      currentProgress.value.delete(event.payload.ruleId)
      currentProgress.value = new Map(currentProgress.value)
    })
  }

  const cleanupEventListeners = () => {
    unlistenProgress?.()
    unlistenComplete?.()
    unlistenError?.()
  }

  // =========================================================================
  // Helpers
  // =========================================================================

  const isRuleRunning = (ruleId: string) => syncStatuses.value.has(ruleId)
  const getRuleProgress = (ruleId: string) => currentProgress.value.get(ruleId)
  const getLastResult = (ruleId: string) => lastResults.value.get(ruleId)

  return {
    syncRules,
    syncStatuses,
    lastResults,
    currentProgress,
    loadRulesAsync,
    createRuleAsync,
    updateRuleAsync,
    deleteRuleAsync,
    toggleRuleAsync,
    startRuleAsync,
    triggerSyncNowAsync,
    refreshStatusAsync,
    startEnabledRulesAsync,
    setupEventListeners,
    cleanupEventListeners,
    isRuleRunning,
    getRuleProgress,
    getLastResult,
    reset: () => {
      syncRules.value = []
      syncStatuses.value = new Map()
      lastResults.value = new Map()
      currentProgress.value = new Map()
      cleanupEventListeners()
    },
  }
})
