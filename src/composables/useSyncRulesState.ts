import { invoke } from '@tauri-apps/api/core'
import type { InjectionKey } from 'vue'
import type { SelectHaexSyncRules } from '~/database/schemas'

type SyncRulesPathType = 'direct' | 'relay' | 'unknown' | 'closed'

export interface ConnectionDiagnostics {
  pathType: SyncRulesPathType
  remoteAddr: string | null
  rttMs: number | null
}

export type SyncRulesStateApi = ReturnType<typeof useSyncRulesState>

export const SyncRulesStateKey: InjectionKey<SyncRulesStateApi> = Symbol('SyncRulesState')

export const useSyncRulesStateInject = (): SyncRulesStateApi => {
  const api = inject(SyncRulesStateKey)
  if (!api) throw new Error('SyncRulesStateKey not provided — call useSyncRulesState() in the parent first')
  return api
}

// Active files in stable slot order, capped at MAX_VISIBLE_SLOTS. Files
// with a slot index >= the cap are not shown (they fall under the
// "+N more" indicator, same as before).
const MAX_VISIBLE_SLOTS = 4

export const useSyncRulesState = () => {
  const { t } = useI18n()
  const syncStore = useFileSyncStore()
  const peerStorageStore = usePeerStorageStore()
  const deviceStore = useDeviceStore()

  // --- Expansion + per-rule UI toggles -------------------------------------

  const expandedMap = reactive<Record<string, boolean>>({})
  // Per-rule toggle for showing log entries from all devices vs. only this device.
  // State is local — not persisted across mounts — so the user starts on the
  // (cheaper) device-local view by default.
  const showAllDevicesMap = reactive<Record<string, boolean>>({})

  const onToggleAllDevicesAsync = async (ruleId: string, value: boolean) => {
    showAllDevicesMap[ruleId] = value
    await syncStore.loadRuleLogsAsync([ruleId], { allDevices: value })
    // Rapid toggling can race; if the toggle was flipped again while we were
    // loading, reconcile once with the current value so the displayed log
    // matches the visible toggle state.
    if (showAllDevicesMap[ruleId] !== value) {
      await syncStore.loadRuleLogsAsync([ruleId], { allDevices: !!showAllDevicesMap[ruleId] })
    }
  }

  // --- Auto-expand newly syncing rules -------------------------------------
  //
  // Auto-expand any rule that is actively syncing so users see progress
  // immediately (e.g. on app start when sync resumes automatically).
  // Keying on a stable string of sorted rule IDs means the watch only
  // fires when the *set* of syncing rules changes — not on every 100ms
  // progress emit. Auto-expanding only IDs that are *newly* in the set
  // (not present in `oldVal`) means a user-initiated collapse during an
  // ongoing sync stays collapsed even when *other* rules start or stop
  // syncing alongside it.
  const activeRuleKey = computed(() =>
    Array.from(syncStore.currentProgress.keys()).sort().join(','),
  )
  watch(
    activeRuleKey,
    (newVal, oldVal) => {
      const previous = new Set(
        (oldVal ?? '').split(',').filter(Boolean),
      )
      const current = newVal.split(',').filter(Boolean)
      for (const id of current) {
        if (!previous.has(id)) {
          expandedMap[id] = true
        }
      }
    },
    { immediate: true },
  )

  // --- Stable slot mapping for active files --------------------------------
  //
  // Stable slot mapping for active files. The backend re-orders the active
  // list as parallel transfers start/complete (a finished file is removed,
  // remaining files visually shift up). Pinning each path to a slot index
  // keeps each row anchored: a finishing file frees its slot, and the next
  // new file takes the lowest free slot — so other rows do not move.
  //
  // We also detect cycle restarts (filesDone going backwards = backend started
  // a new sync cycle, e.g. after a failure). On restart we clear the slot map
  // and bump cycleKey so the progress block remounts — that prevents the main
  // progress bar from animating backwards from 17% to 4% via CSS transition.
  const slotMaps = reactive<Record<string, Map<string, number>>>({})
  const lastFilesDone = reactive<Record<string, number>>({})
  const cycleKey = reactive<Record<string, number>>({})

  watch(
    () => syncStore.currentProgress,
    (progressMap) => {
      for (const [ruleId, prog] of progressMap) {
        let map = slotMaps[ruleId]
        if (!map) {
          map = new Map()
          slotMaps[ruleId] = map
        }

        const prevDone = lastFilesDone[ruleId] ?? 0
        if (prog.filesDone < prevDone) {
          // Cycle restart: drop slot bindings, bump remount key
          map.clear()
          cycleKey[ruleId] = (cycleKey[ruleId] ?? 0) + 1
        }
        lastFilesDone[ruleId] = prog.filesDone

        const currentPaths = new Set((prog.activeFiles ?? []).map(f => f.path))
        // Free slots for files no longer active
        for (const path of [...map.keys()]) {
          if (!currentPaths.has(path)) map.delete(path)
        }
        // Assign newly seen files to the lowest free slot
        const used = new Set(map.values())
        for (const file of prog.activeFiles ?? []) {
          if (!map.has(file.path)) {
            let slot = 0
            while (used.has(slot)) slot++
            map.set(file.path, slot)
            used.add(slot)
          }
        }
      }
      // Drop maps for rules no longer syncing
      for (const ruleId of Object.keys(slotMaps)) {
        if (!progressMap.has(ruleId)) {
          Reflect.deleteProperty(slotMaps, ruleId)
          Reflect.deleteProperty(lastFilesDone, ruleId)
          Reflect.deleteProperty(cycleKey, ruleId)
        }
      }
    },
    { deep: true, immediate: true },
  )

  const stableActiveFiles = (ruleId: string) => {
    const prog = syncStore.getRuleProgress(ruleId)
    if (!prog?.activeFiles?.length) return []
    const map = slotMaps[ruleId]
    if (!map) return []
    return prog.activeFiles
      .filter(f => (map.get(f.path) ?? Infinity) < MAX_VISIBLE_SLOTS)
      .slice()
      .sort((a, b) => (map.get(a.path) ?? 0) - (map.get(b.path) ?? 0))
  }

  // --- Connection-type diagnostics (direct vs relay) for peer rules. -------
  //
  // The Tauri command returns Some(diagnostics) only when there is a *live*
  // cached connection — so until a sync has actually run, peer rules will
  // show "unknown" rather than direct/relay. We poll periodically; the
  // per-sync emit also triggers a refresh so the badge updates as soon as
  // a transfer establishes a connection.

  const connectionMap = ref<Record<string, ConnectionDiagnostics | null>>({})
  const peerEndpointId = (rule: SelectHaexSyncRules): string | null => {
    for (const side of ['sourceConfig', 'targetConfig'] as const) {
      const type = side === 'sourceConfig' ? rule.sourceType : rule.targetType
      if (type !== 'peer') continue
      const cfg = rule[side] as Record<string, unknown> | null
      const id = cfg?.endpointId as string | undefined
      if (id) return id
    }
    return null
  }

  const refreshConnectionDiagnostics = async () => {
    for (const rule of syncStore.syncRules) {
      const nodeId = peerEndpointId(rule)
      if (!nodeId) continue
      try {
        const diag = await invoke<ConnectionDiagnostics | null>(
          'peer_storage_diagnose_connection',
          { nodeId },
        )
        connectionMap.value = { ...connectionMap.value, [rule.id]: diag }
      } catch {
        // Endpoint not running or peer not yet contacted — silent.
      }
    }
  }

  let diagInterval: ReturnType<typeof setInterval> | null = null
  onMounted(() => {
    refreshConnectionDiagnostics()
    diagInterval = setInterval(refreshConnectionDiagnostics, 10_000)
  })
  onBeforeUnmount(() => {
    if (diagInterval) clearInterval(diagInterval)
  })

  // A sync emit means a fresh connection just opened — refresh once so the
  // badge flips from "unknown" to direct/relay without waiting 10s.
  watch(
    () => syncStore.currentProgress.size,
    () => {
      refreshConnectionDiagnostics()
    },
  )

  const rttTitle = (base: string, diag: ConnectionDiagnostics): string => {
    const parts = [base]
    if (diag.rttMs != null) parts.push(`RTT ${diag.rttMs.toFixed(1)} ms`)
    if (diag.remoteAddr) parts.push(diag.remoteAddr)
    return parts.join(' · ')
  }

  const connectionBadge = (rule: SelectHaexSyncRules) => {
    if (!peerEndpointId(rule)) return null
    const diag = connectionMap.value[rule.id]
    if (!diag) {
      return {
        color: 'neutral' as const,
        icon: 'i-lucide-circle-help',
        label: t('connection.unknown'),
        title: t('connection.unknownTitle'),
      }
    }
    switch (diag.pathType) {
      case 'direct':
        return {
          color: 'success' as const,
          icon: 'i-lucide-zap',
          label: t('connection.direct'),
          title: rttTitle(t('connection.directTitle'), diag),
        }
      case 'relay':
        return {
          color: 'warning' as const,
          icon: 'i-lucide-route',
          label: t('connection.relay'),
          title: rttTitle(t('connection.relayTitle'), diag),
        }
      case 'closed':
        return {
          color: 'neutral' as const,
          icon: 'i-lucide-circle-slash',
          label: t('connection.closed'),
          title: t('connection.closedTitle'),
        }
      default:
        return {
          color: 'neutral' as const,
          icon: 'i-lucide-circle-help',
          label: t('connection.unknown'),
          title: t('connection.unknownTitle'),
        }
    }
  }

  // --- Formatters ----------------------------------------------------------

  const formatBytes = (bytes: number): string => {
    if (bytes < 1024) return `${bytes} B`
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`
    if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`
    return `${(bytes / (1024 * 1024 * 1024)).toFixed(2)} GB`
  }

  const formatSpeed = (bytesPerSecond: number): string => {
    if (bytesPerSecond === 0) return t('progress.calculating')
    return `${formatBytes(bytesPerSecond)}/s`
  }

  const formatPercent = (value: number, max: number): string => {
    if (max <= 0) return '0%'
    const pct = Math.min(100, Math.max(0, (value / max) * 100))
    return `${pct.toFixed(pct >= 10 ? 0 : 1)}%`
  }

  const percentValue = (value: number, max: number): number => {
    if (max <= 0) return 0
    return Math.min(100, Math.max(0, (value / max) * 100))
  }

  const providerIcon = (type: string): string => {
    switch (type) {
      case 'local': return 'i-lucide-folder'
      case 'peer': return 'i-lucide-monitor-smartphone'
      case 'cloud': return 'i-lucide-cloud'
      default: return 'i-lucide-file'
    }
  }

  const resolveDeviceName = (type: string, config: unknown): string | null => {
    if (type === 'local') {
      return deviceStore.deviceName || deviceStore.hostname || null
    }
    if (type === 'peer') {
      const cfg = config as Record<string, unknown>
      const endpointId = cfg?.endpointId as string
      if (!endpointId) return null
      const device = peerStorageStore.spaceDevices.find(d => d.endpointId === endpointId)
      return device?.name || endpointId.slice(0, 16) + '...'
    }
    return null
  }

  const formatProviderLabel = (type: string, config: unknown): string => {
    const cfg = config as Record<string, unknown>
    switch (type) {
      case 'local': {
        const path = (cfg?.path as string) || ''
        return path.split(/[/\\]/).pop() || path
      }
      case 'peer': {
        const path = (cfg?.path as string) || ''
        const id = (cfg?.endpointId as string) || ''
        return path || id.slice(0, 12) + '...'
      }
      case 'cloud': {
        const prefix = (cfg?.prefix as string) || '/'
        return `S3:${prefix}`
      }
      default:
        return type
    }
  }

  const formatInterval = (seconds: number): string => {
    if (seconds === 0) return t('intervals.manual')
    if (seconds < 60) return `${seconds}s`
    if (seconds < 3600) return `${seconds / 60} min`
    return `${seconds / 3600}h`
  }

  const hasErrorInLog = (ruleId: string): boolean =>
    syncStore.getRuleLog(ruleId).some(entry => entry.level === 'error')

  const statusLabel = (rule: SelectHaexSyncRules): string => {
    if (!rule.enabled) {
      // If we've seen this rule produce an error in this session, treat the
      // disabled flag as an auto-pause (vs. a manual user pause).
      return syncStore.lastErrors.has(rule.id)
        ? t('status.autoPaused')
        : t('status.paused')
    }
    return syncStore.isRuleRunning(rule.id) ? t('status.running') : t('status.stopped')
  }

  const badgeColor = (rule: SelectHaexSyncRules) => {
    if (!rule.enabled) {
      return syncStore.lastErrors.has(rule.id) ? 'error' : 'warning'
    }
    return syncStore.isRuleRunning(rule.id) ? 'success' : 'neutral'
  }

  const badgeTitle = (rule: SelectHaexSyncRules): string => {
    if (!rule.enabled && syncStore.lastErrors.has(rule.id)) {
      return t('status.autoPausedTitle')
    }
    return ''
  }

  const formatRelative = (timestamp: number): string => {
    const diff = Date.now() - timestamp
    if (diff < 60_000) return t('log.justNow')
    if (diff < 3_600_000) return t('log.minutesAgo', { n: Math.floor(diff / 60_000) })
    if (diff < 86_400_000) return t('log.hoursAgo', { n: Math.floor(diff / 3_600_000) })
    return new Date(timestamp).toLocaleString()
  }

  const formatDeleteMode = (mode: string): string => {
    switch (mode) {
      case 'trash': return t('deleteModes.trash')
      case 'permanent': return t('deleteModes.permanent')
      case 'ignore': return t('deleteModes.ignore')
      default: return mode
    }
  }

  const otherDeviceName = (deviceId: string | null | undefined): string | null => {
    if (!deviceId) return null
    if (deviceId === deviceStore.deviceId) return null
    return deviceStore.getDeviceName(deviceId)
  }

  return {
    // state
    expandedMap,
    showAllDevicesMap,
    cycleKey,
    // helpers
    stableActiveFiles,
    peerEndpointId,
    connectionBadge,
    refreshConnectionDiagnostics,
    onToggleAllDevicesAsync,
    formatBytes,
    formatSpeed,
    formatPercent,
    percentValue,
    providerIcon,
    resolveDeviceName,
    formatProviderLabel,
    formatInterval,
    hasErrorInLog,
    statusLabel,
    badgeColor,
    badgeTitle,
    formatRelative,
    formatDeleteMode,
    otherDeviceName,
  }
}
