import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { eq } from 'drizzle-orm'
import { haexSyncRules, haexSyncState, type SelectHaexSyncRules } from '~/database/schemas'
import { subscribeToSyncUpdates, unsubscribeFromSyncUpdates } from '~/stores/sync/syncEvents'
import { createLogger } from '@/stores/logging'
import { requireDb } from '~/stores/vault'

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

interface FileProgress {
  path: string
  bytesDone: number
  bytesTotal: number
}

export interface SyncLogEntry {
  /** Wall-clock time the entry was recorded */
  at: number
  level: 'error' | 'info'
  /** Short, deduped user-facing message */
  summary: string
  /** Raw error text from backend (kept for diagnostics) */
  raw?: string
  /** How many times this same root-cause has fired in a row */
  repeats?: number
}

const MAX_LOG_ENTRIES_PER_RULE = 50

interface SyncProgress {
  currentFile: string
  filesDone: number
  filesTotal: number
  bytesDone: number
  bytesTotal: number
  activeFiles: FileProgress[]
  bytesPerSecond: number
}

const log = createLogger('FILE_SYNC')

/**
 * Pull the most useful line out of a wrapped backend error.
 * Backend errors look like:
 *   "Provider error: Provider error: Internal error: S3 list failed: Got HTTP 404 with content '<?xml...'"
 * For toasts we strip the wrapper prefixes and the XML envelope so the user
 * sees something readable like "The specified bucket does not exist".
 */
function extractUserFacingError(raw: string): string {
  // Try to pull <Message>…</Message> out of any embedded S3 XML.
  const messageMatch = raw.match(/<Message>([^<]+)<\/Message>/)
  if (messageMatch) return messageMatch[1]!

  // Strip repeated "<X> error:" prefixes.
  const stripped = raw.replace(/^(?:[A-Za-z ]+error:\s*)+/, '')
  // Cap length so a giant blob doesn't blow up the toast.
  return stripped.length > 240 ? stripped.slice(0, 237) + '…' : stripped
}

/**
 * Build a *stable* signature of an error for deduplication.
 * Crucially we must ignore S3 `<RequestId>` (changes on every retry) and any
 * other request-correlated tokens, otherwise repeated failures with identical
 * root cause look like distinct errors and the dedup is useless.
 */
function errorSignature(raw: string): string {
  // For S3 XML errors, use Code + Message (stable across retries).
  const codeMatch = raw.match(/<Code>([^<]+)<\/Code>/)
  const messageMatch = raw.match(/<Message>([^<]+)<\/Message>/)
  if (codeMatch || messageMatch) {
    return `s3:${codeMatch?.[1] ?? ''}:${messageMatch?.[1] ?? ''}`
  }
  // Strip volatile tokens (request ids, durations) before comparing.
  return raw
    .replace(/RequestId[^,\s)']*/gi, '')
    .replace(/\b\d+(?:\.\d+)?(?:ms|s|m|h)\b/g, '')
    .replace(/\s+/g, ' ')
    .trim()
}

export const useFileSyncStore = defineStore('fileSyncStore', () => {
  const { currentVault } = storeToRefs(useVaultStore())
  const { add: addToast } = useToast()

  const syncRules = ref<SelectHaexSyncRules[]>([])
  const syncStatuses = ref<Map<string, SyncRuleStatus>>(new Map())
  const lastResults = ref<Map<string, SyncResult>>(new Map())
  const lastErrors = ref<Map<string, string>>(new Map())
  const currentProgress = ref<Map<string, SyncProgress>>(new Map())
  // In-memory rolling log of recent events per rule. Capped per rule to
  // keep memory bounded. Surfaces in the UI as a "history" — not persisted
  // across app restarts (would need a dedicated table for that).
  const ruleLogs = ref<Map<string, SyncLogEntry[]>>(new Map())

  const appendLogEntry = (ruleId: string, entry: SyncLogEntry) => {
    const list = ruleLogs.value.get(ruleId) ?? []
    list.unshift(entry)
    if (list.length > MAX_LOG_ENTRIES_PER_RULE) {
      list.length = MAX_LOG_ENTRIES_PER_RULE
    }
    ruleLogs.value.set(ruleId, list)
    ruleLogs.value = new Map(ruleLogs.value)
  }

  const getRuleLog = (ruleId: string): SyncLogEntry[] =>
    ruleLogs.value.get(ruleId) ?? []

  const clearRuleLog = (ruleId: string) => {
    if (ruleLogs.value.delete(ruleId)) {
      ruleLogs.value = new Map(ruleLogs.value)
    }
  }
  // Track lastSyncedAt per rule to detect remote changes
  const knownSyncTimestamps = new Map<string, number | null>()

  // =========================================================================
  // CRUD operations via Drizzle
  // =========================================================================

  const loadRulesAsync = async () => {
    const db = requireDb()
    syncRules.value = await db.select().from(haexSyncRules).all()
    // Seed the timestamp cache so we only trigger on actual changes
    for (const rule of syncRules.value) {
      if (!knownSyncTimestamps.has(rule.id)) {
        knownSyncTimestamps.set(rule.id, rule.lastSyncedAt ?? null)
      }
    }
  }

  const createRuleAsync = async (rule: typeof haexSyncRules.$inferInsert) => {
    const db = requireDb()
    await db.insert(haexSyncRules).values(rule)
    await loadRulesAsync()
    // Start sync immediately
    const created = syncRules.value.find(r => r.id === rule.id)
    if (created?.enabled) {
      await startRuleAsync(created)
    }
  }

  const updateRuleAsync = async (id: string, updates: Partial<typeof haexSyncRules.$inferInsert>) => {
    const db = requireDb()
    await db.update(haexSyncRules).set(updates).where(eq(haexSyncRules.id, id))
    await loadRulesAsync()
  }

  const deleteRuleAsync = async (id: string) => {
    const db = requireDb()
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
    log.info(`Starting rule ${rule.id}: ${rule.sourceType} → ${rule.targetType}, interval=${rule.syncIntervalSeconds}s`)
    await invoke('file_sync_start_rule', {
      ruleId: rule.id,
      sourceType: rule.sourceType,
      sourceConfig: typeof rule.sourceConfig === 'string' ? JSON.parse(rule.sourceConfig) : rule.sourceConfig,
      targetType: rule.targetType,
      targetConfig: typeof rule.targetConfig === 'string' ? JSON.parse(rule.targetConfig) : rule.targetConfig,
      direction: rule.direction,
      deleteMode: rule.deleteMode,
      intervalSeconds: rule.syncIntervalSeconds,
    })
  }

  const triggerSyncNowAsync = async (ruleId: string) => {
    const rule = syncRules.value.find(r => r.id === ruleId)
    if (!rule) throw new Error('Rule not found')

    // Refresh before checking — the in-memory cache may be stale if the loop
    // started/stopped since the last status poll.
    await refreshStatusAsync()

    // If the sync loop is already running, poke its trigger channel and return
    // immediately — the loop emits progress/complete events as usual.
    // Avoids blocking the UI thread for the full transfer duration.
    if (isRuleRunning(ruleId)) {
      await invoke('file_sync_trigger_by_watcher', { ruleId })
      return null
    }

    // Rule not running: one-shot blocking sync
    const result = await invoke<SyncResult>('file_sync_trigger_now', {
      ruleId: rule.id,
      sourceType: rule.sourceType,
      sourceConfig: typeof rule.sourceConfig === 'string' ? JSON.parse(rule.sourceConfig) : rule.sourceConfig,
      targetType: rule.targetType,
      targetConfig: typeof rule.targetConfig === 'string' ? JSON.parse(rule.targetConfig) : rule.targetConfig,
      direction: rule.direction,
      deleteMode: rule.deleteMode,
    })
    lastResults.value.set(ruleId, result)
    lastResults.value = new Map(lastResults.value)
    return result
  }

  const refreshStatusAsync = async () => {
    const statuses = await invoke<SyncRuleStatus[]>('file_sync_status')
    syncStatuses.value = new Map(statuses.map(s => [s.ruleId, s]))
  }

  // Stop and re-start a rule so it picks up the latest config from DB
  // (e.g. after a referenced storage backend was updated).
  const restartRuleAsync = async (rule: SelectHaexSyncRules) => {
    try { await invoke('file_sync_stop_rule', { ruleId: rule.id }) } catch { /* not running */ }
    if (rule.enabled) await startRuleAsync(rule)
  }

  // Restart every enabled rule that references the given storage backend id
  // in its source or target config. Used after editing a cloud backend so
  // running sync loops pick up the new credentials/region/endpoint.
  const restartRulesUsingBackendAsync = async (backendId: string): Promise<number> => {
    await loadRulesAsync()
    const affected = syncRules.value.filter((rule) => {
      const src = rule.sourceConfig as Record<string, unknown> | null
      const tgt = rule.targetConfig as Record<string, unknown> | null
      return (
        (rule.sourceType === 'cloud' && src?.backendId === backendId) ||
        (rule.targetType === 'cloud' && tgt?.backendId === backendId)
      )
    })
    for (const rule of affected) {
      try {
        await restartRuleAsync(rule)
      } catch (error) {
        log.warn(`Failed to restart rule ${rule.id} after backend update:`, error)
      }
    }
    return affected.length
  }

  // Auto-start all enabled rules relevant to this device
  const startEnabledRulesAsync = async () => {
    for (const rule of syncRules.value) {
      if (!rule.enabled) continue
      try {
        await startRuleAsync(rule)
      } catch (e) {
        log.warn(`Failed to start rule ${rule.id}:`, e)
      }
    }
  }

  // =========================================================================
  // Event listeners
  // =========================================================================

  let unlistenProgress: (() => void) | null = null
  let unlistenComplete: (() => void) | null = null
  let unlistenError: (() => void) | null = null
  let unlistenAutoPaused: (() => void) | null = null

  const setupEventListeners = async () => {
    if (unlistenProgress || unlistenComplete || unlistenError) return

    // Backend emits these via emit_to("main", …) — pin the listener
    // explicitly so Tauri v2 routes them through (default-Any is dropped
    // in production builds).
    unlistenProgress = await listen<{ ruleId: string } & SyncProgress>(
      'file-sync:progress',
      (event) => {
        currentProgress.value.set(event.payload.ruleId, event.payload)
        currentProgress.value = new Map(currentProgress.value)
      },
      { target: 'main' },
    )

    unlistenComplete = await listen<{ ruleId: string; result: SyncResult }>(
      'file-sync:complete',
      (event) => {
        const { ruleId, result } = event.payload
        lastResults.value.set(ruleId, result)
        lastResults.value = new Map(lastResults.value)
        currentProgress.value.delete(ruleId)
        currentProgress.value = new Map(currentProgress.value)

        // Successful sync clears the dedup signature and adds an info entry
        // to the rolling log so users can see the timeline (incl. recovery).
        const hadError = lastErrors.value.has(ruleId)
        if (hadError) {
          lastErrors.value.delete(ruleId)
          lastErrors.value = new Map(lastErrors.value)
          appendLogEntry(ruleId, {
            at: Date.now(),
            level: 'info',
            summary: `Sync erfolgreich (nach vorherigem Fehler) — ${result.filesDownloaded} Datei(en), ${result.bytesTransferred} Bytes`,
          })
        } else if (
          result.filesDownloaded > 0 ||
          result.filesDeleted > 0 ||
          result.directoriesCreated > 0
        ) {
          // Only log non-trivial successful cycles to avoid spamming the
          // history with empty no-op syncs.
          appendLogEntry(ruleId, {
            at: Date.now(),
            level: 'info',
            summary: `Sync erfolgreich — ${result.filesDownloaded} Datei(en), ${result.bytesTransferred} Bytes`,
          })
        }
      },
      { target: 'main' },
    )

    unlistenError = await listen<{ ruleId: string; error: string }>(
      'file-sync:error',
      (event) => {
        const { ruleId, error } = event.payload
        const sig = errorSignature(error)
        const previousSig = lastErrors.value.get(ruleId)
        lastErrors.value.set(ruleId, sig)
        lastErrors.value = new Map(lastErrors.value)
        currentProgress.value.delete(ruleId)
        currentProgress.value = new Map(currentProgress.value)

        const summary = extractUserFacingError(error)

        // Update the rolling log: collapse identical consecutive root-causes
        // into a single entry with a repeat counter instead of growing the
        // list one entry per retry.
        const existing = ruleLogs.value.get(ruleId) ?? []
        const head = existing[0]
        if (head && head.level === 'error' && errorSignature(head.raw ?? head.summary) === sig) {
          head.repeats = (head.repeats ?? 1) + 1
          head.at = Date.now()
          ruleLogs.value = new Map(ruleLogs.value)
        } else {
          appendLogEntry(ruleId, {
            at: Date.now(),
            level: 'error',
            summary,
            raw: error,
            repeats: 1,
          })
        }

        // Only surface the error the first time it appears (or when its
        // root cause changes). Repeated identical failures from the retry
        // loop — including S3 errors with rotating RequestIds — are
        // silenced to avoid console/toast spam.
        if (previousSig !== sig) {
          log.error(`Rule ${ruleId} error:`, error)
          const rule = syncRules.value.find(r => r.id === ruleId)
          addToast({
            title: rule ? `Sync-Fehler: ${rule.id.slice(0, 8)}` : 'Sync-Fehler',
            description: summary,
            color: 'error',
            duration: 8000,
          })
        }
      },
      { target: 'main' },
    )

    unlistenAutoPaused = await listen<{
      ruleId: string
      consecutiveFailures: number
      lastError: string
    }>(
      'file-sync:auto-paused',
      async (event) => {
        const { ruleId, consecutiveFailures, lastError } = event.payload
        const summary = extractUserFacingError(lastError)
        log.warn(`Rule ${ruleId} auto-paused after ${consecutiveFailures} failures`)
        appendLogEntry(ruleId, {
          at: Date.now(),
          level: 'error',
          summary: `Auto-pausiert nach ${consecutiveFailures} Fehlversuchen: ${summary}`,
          raw: lastError,
        })
        addToast({
          title: 'Sync-Regel pausiert',
          description: `Nach ${consecutiveFailures} fehlgeschlagenen Versuchen wurde die Regel automatisch deaktiviert. Letzter Fehler: ${summary}`,
          color: 'warning',
          duration: 0,
        })
        // The backend already updated `enabled = false` in the DB; refresh
        // our local copy so the UI reflects the paused state.
        try { await loadRulesAsync() } catch { /* best effort */ }
      },
      { target: 'main' },
    )

    // Subscribe to CRDT changes on sync_rules table.
    // When a remote device syncs and updates lastSyncedAt, trigger only affected rules.
    subscribeToSyncUpdates('file-sync', ['haex_sync_rules'], async () => {
      await loadRulesAsync()

      for (const rule of syncRules.value) {
        const knownTimestamp = knownSyncTimestamps.get(rule.id)
        const currentTimestamp = rule.lastSyncedAt
        if (currentTimestamp && currentTimestamp !== knownTimestamp) {
          knownSyncTimestamps.set(rule.id, currentTimestamp)
          log.info(`Remote sync detected for rule ${rule.id}, triggering local sync`)
          try {
            await invoke('file_sync_trigger_by_watcher', { ruleId: rule.id })
          } catch {
            // Rule might not be running locally — that's fine
          }
        }
      }
    })
  }

  const cleanupEventListeners = () => {
    unlistenProgress?.()
    unlistenComplete?.()
    unlistenError?.()
    unlistenAutoPaused?.()
    unlistenProgress = null
    unlistenComplete = null
    unlistenError = null
    unlistenAutoPaused = null
    unsubscribeFromSyncUpdates('file-sync')
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
    lastErrors,
    ruleLogs,
    getRuleLog,
    clearRuleLog,
    currentProgress,
    loadRulesAsync,
    createRuleAsync,
    updateRuleAsync,
    deleteRuleAsync,
    toggleRuleAsync,
    startRuleAsync,
    restartRuleAsync,
    restartRulesUsingBackendAsync,
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
      lastErrors.value = new Map()
      ruleLogs.value = new Map()
      currentProgress.value = new Map()
      cleanupEventListeners()
    },
  }
})
