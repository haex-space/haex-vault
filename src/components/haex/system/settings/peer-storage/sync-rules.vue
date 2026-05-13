<template>
  <HaexSystemSettingsLayout
    :title="t('title')"
    show-back
    @back="$emit('back')"
  >
    <template #description>
      {{ t('description') }}
    </template>
    <template #actions>
      <UiButton
        icon="i-lucide-plus"
        color="primary"
        @click="editingRule = null; showCreateDialog = true"
      >
        {{ t('addRule') }}
      </UiButton>
    </template>

    <!-- Empty state -->
    <HaexSystemSettingsLayoutEmpty
      v-if="!syncStore.syncRules.length"
      :message="t('empty')"
      icon="i-lucide-refresh-cw-off"
    />

    <!-- Rules cards -->
    <div v-else class="space-y-4">
      <UCard
        v-for="rule in syncStore.syncRules"
        :key="rule.id"
        :class="{ 'opacity-50': !rule.enabled }"
      >
        <UCollapsible v-model:open="expandedMap[rule.id]">
          <!-- Always-visible: badges + source/target -->
          <div>
            <!-- Toggle area: clicks on header or body bubble to the
                 outer wrapper, which Nuxt UI binds as the CollapsibleTrigger
                 (default slot is wrapped in `<CollapsibleTrigger as-child>`),
                 so the accordion toggles automatically. The action footer
                 below stops propagation so its buttons don't toggle. -->
            <div class="cursor-pointer">
              <!-- Header: badges + expand toggle -->
              <div class="flex items-center gap-2 mb-3">
                <UBadge
                  :color="badgeColor(rule)"
                  variant="subtle"
                  size="sm"
                  :title="badgeTitle(rule)"
                >
                  <UIcon
                    v-if="!rule.enabled"
                    name="i-lucide-pause"
                    class="w-3 h-3"
                  />
                  {{ statusLabel(rule) }}
                </UBadge>
                <UBadge variant="subtle" color="neutral" size="sm">
                  {{ rule.direction === 'two_way' ? t('direction.twoWay') : t('direction.oneWay') }}
                </UBadge>
                <UBadge variant="subtle" color="neutral" size="sm">
                  <UIcon name="i-lucide-clock" class="w-3 h-3" />
                  {{ formatInterval(rule.syncIntervalSeconds) }}
                </UBadge>
                <UBadge variant="subtle" color="neutral" size="sm">
                  <UIcon name="i-lucide-trash-2" class="w-3 h-3" />
                  {{ formatDeleteMode(rule.deleteMode) }}
                </UBadge>
                <UBadge
                  v-if="connectionBadge(rule)"
                  :color="connectionBadge(rule)!.color"
                  variant="subtle"
                  size="sm"
                  :title="connectionBadge(rule)!.title"
                >
                  <UIcon :name="connectionBadge(rule)!.icon" class="w-3 h-3" />
                  {{ connectionBadge(rule)!.label }}
                </UBadge>
                <UIcon
                  name="i-lucide-chevron-down"
                  class="w-4 h-4 text-muted ml-auto shrink-0 transition-transform duration-200"
                  :class="{ 'rotate-180': expandedMap[rule.id] }"
                />
              </div>

              <!-- Body: source → target -->
              <div class="flex items-center gap-3">
                <!-- Source -->
                <div class="flex-1 min-w-0">
                  <div class="flex items-center gap-2 mb-1">
                    <UIcon :name="providerIcon(rule.sourceType)" class="w-4 h-4 text-muted shrink-0" />
                    <span class="text-xs text-muted">{{ t('label.source') }}</span>
                  </div>
                  <p class="text-sm font-medium truncate">
                    {{ formatProviderLabel(rule.sourceType, rule.sourceConfig) }}
                  </p>
                  <p v-if="resolveDeviceName(rule.sourceType, rule.sourceConfig)" class="text-xs text-muted truncate">
                    {{ resolveDeviceName(rule.sourceType, rule.sourceConfig) }}
                  </p>
                </div>

                <!-- Arrow -->
                <UIcon
                  :name="rule.direction === 'two_way' ? 'i-lucide-arrow-left-right' : 'i-lucide-arrow-right'"
                  class="w-5 h-5 text-primary shrink-0"
                />

                <!-- Target -->
                <div class="flex-1 min-w-0">
                  <div class="flex items-center gap-2 mb-1">
                    <UIcon :name="providerIcon(rule.targetType)" class="w-4 h-4 text-muted shrink-0" />
                    <span class="text-xs text-muted">{{ t('label.target') }}</span>
                  </div>
                  <p class="text-sm font-medium truncate">
                    {{ formatProviderLabel(rule.targetType, rule.targetConfig) }}
                  </p>
                  <p v-if="resolveDeviceName(rule.targetType, rule.targetConfig)" class="text-xs text-muted truncate">
                    {{ resolveDeviceName(rule.targetType, rule.targetConfig) }}
                  </p>
                </div>
              </div>
            </div>

            <!-- Footer: actions (outside toggle area; clicks here must not
                 expand/collapse the card) -->
            <div
              class="flex items-center justify-end gap-1 mt-3 pt-3 border-t border-default"
              @click.stop
            >
              <UiButton
                icon="i-lucide-refresh-cw"
                variant="ghost"
                color="neutral"
                :loading="isSyncing === rule.id"
                @click="onSyncNowAsync(rule.id)"
              />
              <UChip
                :show="syncStore.getRuleLog(rule.id).length > 0"
                :text="syncStore.getRuleLog(rule.id).length"
                :color="hasErrorInLog(rule.id) ? 'error' : 'primary'"
                size="sm"
              >
                <UiButton
                  icon="i-lucide-scroll-text"
                  variant="ghost"
                  :color="hasErrorInLog(rule.id) ? 'error' : 'neutral'"
                  :title="t('actions.viewLog')"
                  @click="expandedMap[rule.id] = true"
                />
              </UChip>
              <UiButton
                icon="i-lucide-pencil"
                variant="ghost"
                color="neutral"
                @click="onEdit(rule)"
              />
              <USwitch
                :model-value="rule.enabled"
                @update:model-value="(val: boolean) => onToggleAsync(rule.id, val)"
              />
              <UiButton
                icon="i-lucide-trash-2"
                variant="ghost"
                color="error"
                @click="onDeleteAsync(rule.id)"
              />
            </div>
          </div>

          <!-- Collapsible: progress + last result -->
          <template #content>
            <div class="mt-3 pt-3 border-t border-default">
              <!-- Active sync progress.
                   :key changes when a cycle restart is detected (filesDone
                   regresses), forcing remount so the bar doesn't animate
                   backwards from the old high to the new low. -->
              <div
                v-if="syncStore.getRuleProgress(rule.id)"
                :key="`progress-${rule.id}-${cycleKey[rule.id] ?? 0}`"
                class="space-y-2"
              >
                <!-- Stats row: active + done counts + speed -->
                <div class="flex items-center justify-between text-xs">
                  <span class="text-muted">
                    <span
                      v-if="syncStore.getRuleProgress(rule.id)!.activeFiles?.length"
                      class="text-primary font-medium"
                    >
                      {{ syncStore.getRuleProgress(rule.id)!.activeFiles.length }} {{ t('progress.active') }}
                    </span>
                    <span v-if="syncStore.getRuleProgress(rule.id)!.activeFiles?.length && syncStore.getRuleProgress(rule.id)!.filesDone > 0"> · </span>
                    <span v-if="syncStore.getRuleProgress(rule.id)!.filesDone > 0">
                      {{ syncStore.getRuleProgress(rule.id)!.filesDone }}/{{ syncStore.getRuleProgress(rule.id)!.filesTotal }} {{ t('progress.done') }}
                    </span>
                  </span>
                  <span v-if="syncStore.getRuleProgress(rule.id)!.bytesPerSecond > 0" class="text-primary font-medium shrink-0 ml-2 tabular-nums">
                    {{ formatSpeed(syncStore.getRuleProgress(rule.id)!.bytesPerSecond) }}
                  </span>
                </div>
                <!-- Determinate progress bar (explicit DIVs — UProgress had
                     an animated indeterminate look that read like a spinner). -->
                <div class="h-2 w-full rounded-full bg-elevated overflow-hidden">
                  <div
                    class="h-full bg-primary transition-[width] duration-150 ease-linear"
                    :style="{ width: percentValue(
                      syncStore.getRuleProgress(rule.id)!.bytesTotal > 0
                        ? syncStore.getRuleProgress(rule.id)!.bytesDone
                        : syncStore.getRuleProgress(rule.id)!.filesDone,
                      syncStore.getRuleProgress(rule.id)!.bytesTotal > 0
                        ? syncStore.getRuleProgress(rule.id)!.bytesTotal
                        : syncStore.getRuleProgress(rule.id)!.filesTotal
                    ) + '%' }"
                  />
                </div>
                <!-- Bytes transferred + percentage -->
                <div class="flex items-center justify-between text-xs tabular-nums">
                  <span v-if="syncStore.getRuleProgress(rule.id)!.bytesTotal > 0" class="text-muted">
                    {{ formatBytes(syncStore.getRuleProgress(rule.id)!.bytesDone) }} / {{ formatBytes(syncStore.getRuleProgress(rule.id)!.bytesTotal) }}
                  </span>
                  <span v-else class="text-muted">
                    {{ syncStore.getRuleProgress(rule.id)!.filesDone }} / {{ syncStore.getRuleProgress(rule.id)!.filesTotal }}
                  </span>
                  <span class="text-primary font-medium">
                    {{ formatPercent(
                      syncStore.getRuleProgress(rule.id)!.bytesTotal > 0
                        ? syncStore.getRuleProgress(rule.id)!.bytesDone
                        : syncStore.getRuleProgress(rule.id)!.filesDone,
                      syncStore.getRuleProgress(rule.id)!.bytesTotal > 0
                        ? syncStore.getRuleProgress(rule.id)!.bytesTotal
                        : syncStore.getRuleProgress(rule.id)!.filesTotal
                    ) }}
                  </span>
                </div>
                <!-- Active files list with per-file progress.
                     Plain list (no TransitionGroup): the previous fade
                     transition with `position: absolute` on leave caused
                     leaving rows to overlay entering rows during parallel
                     batch turnover, which read as flicker. Per-bar width
                     transitions still smooth byte progress animation.
                     Files are iterated in stable slot order, so a finishing
                     file does not push the remaining ones up — the next
                     new file takes the freed slot in place. -->
                <div
                  v-if="stableActiveFiles(rule.id).length"
                  class="mt-1 space-y-1.5"
                >
                  <div
                    v-for="fp in stableActiveFiles(rule.id)"
                    :key="fp.path"
                    class="space-y-0.5"
                  >
                    <div class="flex items-center gap-1.5 text-xs text-muted">
                      <UIcon
                        :name="fp.bytesTotal > 0 && fp.bytesDone >= fp.bytesTotal
                          ? 'i-lucide-check'
                          : 'i-lucide-arrow-down'"
                        class="w-3 h-3 text-primary shrink-0"
                      />
                      <span class="truncate flex-1">{{ fp.path.split(/[/\\]/).pop() }}</span>
                      <span v-if="fp.bytesTotal > 0" class="shrink-0 tabular-nums">
                        {{ formatBytes(fp.bytesDone) }} / {{ formatBytes(fp.bytesTotal) }}
                      </span>
                      <span v-if="fp.bytesTotal > 0" class="shrink-0 tabular-nums text-primary font-medium">
                        {{ fp.bytesDone >= fp.bytesTotal ? t('progress.finalizing') : formatPercent(fp.bytesDone, fp.bytesTotal) }}
                      </span>
                    </div>
                    <div
                      v-if="fp.bytesTotal > 0"
                      class="h-1 w-full rounded-full bg-elevated overflow-hidden"
                    >
                      <div
                        class="h-full bg-primary transition-[width] duration-150 ease-linear"
                        :style="{ width: percentValue(fp.bytesDone, fp.bytesTotal) + '%' }"
                      />
                    </div>
                  </div>
                  <div
                    v-if="(syncStore.getRuleProgress(rule.id)!.activeFiles?.length ?? 0) > stableActiveFiles(rule.id).length"
                    class="text-xs text-muted"
                  >
                    +{{ (syncStore.getRuleProgress(rule.id)!.activeFiles?.length ?? 0) - stableActiveFiles(rule.id).length }} {{ t('progress.moreFiles') }}
                  </div>
                </div>
              </div>

              <!-- Last sync result -->
              <div v-else-if="syncStore.getLastResult(rule.id)">
                <div class="text-xs text-muted mb-2">{{ t('lastSync.title') }}</div>
                <div class="flex flex-wrap gap-x-4 gap-y-1 text-xs">
                  <span v-if="syncStore.getLastResult(rule.id)!.filesDownloaded > 0" class="flex items-center gap-1">
                    <UIcon name="i-lucide-download" class="w-3 h-3 text-primary" />
                    {{ syncStore.getLastResult(rule.id)!.filesDownloaded }} {{ t('lastSync.downloaded') }}
                  </span>
                  <span v-if="syncStore.getLastResult(rule.id)!.filesDeleted > 0" class="flex items-center gap-1">
                    <UIcon name="i-lucide-trash-2" class="w-3 h-3 text-muted" />
                    {{ syncStore.getLastResult(rule.id)!.filesDeleted }} {{ t('lastSync.deleted') }}
                  </span>
                  <span v-if="syncStore.getLastResult(rule.id)!.bytesTransferred > 0" class="flex items-center gap-1">
                    <UIcon name="i-lucide-hard-drive" class="w-3 h-3 text-muted" />
                    {{ formatBytes(syncStore.getLastResult(rule.id)!.bytesTransferred) }}
                  </span>
                  <span
                    v-if="syncStore.getLastResult(rule.id)!.filesDownloaded === 0 && syncStore.getLastResult(rule.id)!.filesDeleted === 0 && syncStore.getLastResult(rule.id)!.bytesTransferred === 0 && syncStore.getLastResult(rule.id)!.directoriesCreated === 0 && syncStore.getLastResult(rule.id)!.conflictsResolved === 0"
                    class="text-muted"
                  >
                    {{ t('lastSync.upToDate') }}
                  </span>
                </div>
                <div v-if="syncStore.getLastResult(rule.id)!.errors.length > 0" class="mt-2 space-y-1">
                  <p v-for="err in syncStore.getLastResult(rule.id)!.errors.slice(0, 3)" :key="err" class="text-xs text-error truncate">
                    {{ err }}
                  </p>
                  <p v-if="syncStore.getLastResult(rule.id)!.errors.length > 3" class="text-xs text-muted">
                    +{{ syncStore.getLastResult(rule.id)!.errors.length - 3 }} {{ t('lastSync.moreErrors') }}
                  </p>
                </div>
              </div>

              <!-- No data yet -->
              <div v-else class="text-xs text-muted">
                {{ t('progress.noData') }}
              </div>

              <!-- Activity log / error history -->
              <div class="mt-3 pt-3 border-t border-default">
                <div class="flex items-center justify-between mb-2 gap-2 flex-wrap">
                  <span class="text-xs text-muted font-medium">{{ t('log.title') }}</span>
                  <div class="flex items-center gap-2">
                    <USwitch
                      :model-value="!!showAllDevicesMap[rule.id]"
                      size="xs"
                      :label="t('log.allDevices')"
                      @update:model-value="(val: boolean) => onToggleAllDevicesAsync(rule.id, val)"
                    />
                    <UiButton
                      v-if="syncStore.getRuleLog(rule.id).length"
                      icon="i-lucide-eraser"
                      variant="ghost"
                      color="neutral"
                      size="xs"
                      @click="syncStore.clearRuleLog(rule.id)"
                    >
                      {{ t('log.clear') }}
                    </UiButton>
                  </div>
                </div>
                <div
                  v-if="syncStore.getRuleLog(rule.id).length"
                  class="space-y-1 max-h-60 overflow-y-auto"
                >
                  <div
                    v-for="(entry, idx) in syncStore.getRuleLog(rule.id)"
                    :key="`${rule.id}-${idx}-${entry.at}`"
                    class="flex items-start gap-2 text-xs"
                  >
                    <UIcon
                      :name="entry.level === 'error' ? 'i-lucide-circle-x' : 'i-lucide-check'"
                      :class="entry.level === 'error' ? 'text-error' : 'text-success'"
                      class="w-3 h-3 mt-0.5 shrink-0"
                    />
                    <div class="flex-1 min-w-0">
                      <div class="flex items-baseline gap-2 flex-wrap">
                        <span
                          class="break-words"
                          :class="entry.level === 'error' ? 'text-error' : ''"
                        >
                          {{ entry.summary }}
                        </span>
                        <UBadge
                          v-if="otherDeviceName(entry.deviceId)"
                          color="neutral"
                          variant="subtle"
                          size="xs"
                          :title="entry.deviceId ?? ''"
                        >
                          <UIcon name="i-lucide-monitor" class="w-3 h-3" />
                          {{ otherDeviceName(entry.deviceId) }}
                        </UBadge>
                        <span
                          v-if="entry.repeats && entry.repeats > 1"
                          class="text-muted shrink-0"
                          :title="t('log.repeats')"
                        >
                          ×{{ entry.repeats }}
                        </span>
                      </div>
                      <span class="text-muted text-[10px]">
                        {{ formatRelative(entry.at) }}
                      </span>
                    </div>
                  </div>
                </div>
                <p v-else class="text-xs text-muted italic">
                  {{ t('log.empty') }}
                </p>
              </div>
            </div>
          </template>
        </UCollapsible>
      </UCard>
    </div>

    <HaexSystemSettingsPeerStorageCreateSyncRuleDialog
      v-model:open="showCreateDialog"
      :edit-rule="editingRule"
      @created="onRuleCreated"
      @updated="onRuleCreated"
    />
  </HaexSystemSettingsLayout>
</template>

<script setup lang="ts">
import { invoke } from '@tauri-apps/api/core'
import type { SelectHaexSyncRules } from '~/database/schemas'

defineEmits<{ back: [] }>()

const { t } = useI18n()
const { add } = useToast()
const syncStore = useFileSyncStore()
const peerStorageStore = usePeerStorageStore()

const showCreateDialog = ref(false)
const editingRule = ref<SelectHaexSyncRules | null>(null)
const isSyncing = ref<string | null>(null)
const expandedMap = reactive<Record<string, boolean>>({})
// Per-rule toggle for showing log entries from all devices vs. only this device.
// State is local — not persisted across mounts — so the user starts on the
// (cheaper) device-local view by default.
const showAllDevicesMap = reactive<Record<string, boolean>>({})

const deviceStore = useDeviceStore()

onMounted(() => {
  // The badge in log entries resolves human-readable device names; without a
  // load the map is empty and we'd only ever fall back to the truncated id.
  deviceStore.loadKnownDevicesAsync().catch(() => { /* best effort */ })
})

const otherDeviceName = (deviceId: string | null | undefined): string | null => {
  if (!deviceId) return null
  if (deviceId === deviceStore.deviceId) return null
  return deviceStore.getDeviceName(deviceId)
}

const onToggleAllDevicesAsync = async (ruleId: string, value: boolean) => {
  showAllDevicesMap[ruleId] = value
  await syncStore.loadRuleLogsAsync([ruleId], { allDevices: value })
}

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
const MAX_VISIBLE_SLOTS = 4
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
        delete slotMaps[ruleId]
        delete lastFilesDone[ruleId]
        delete cycleKey[ruleId]
      }
    }
  },
  { deep: true, immediate: true },
)

// Active files in stable slot order, capped at MAX_VISIBLE_SLOTS. Files
// with a slot index >= the cap are not shown (they fall under the
// "+N more" indicator, same as before).
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

onMounted(async () => {
  await syncStore.loadRulesAsync()
  await syncStore.refreshStatusAsync()
  await peerStorageStore.loadSpaceDevicesAsync()
})

// ---------------------------------------------------------------------------
// Connection-type diagnostics (direct vs relay) for peer rules.
//
// The Tauri command returns Some(diagnostics) only when there is a *live*
// cached connection — so until a sync has actually run, peer rules will
// show "unknown" rather than direct/relay. We poll periodically; the
// per-sync emit also triggers a refresh so the badge updates as soon as
// a transfer establishes a connection.
// ---------------------------------------------------------------------------

type PathType = 'direct' | 'relay' | 'unknown' | 'closed'
interface ConnectionDiagnostics {
  pathType: PathType
  remoteAddr: string | null
  rttMs: number | null
}

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

const rttTitle = (base: string, diag: ConnectionDiagnostics): string => {
  const parts = [base]
  if (diag.rttMs != null) parts.push(`RTT ${diag.rttMs.toFixed(1)} ms`)
  if (diag.remoteAddr) parts.push(diag.remoteAddr)
  return parts.join(' · ')
}

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
    const device = peerStorageStore.spaceDevices.find(d => d.deviceEndpointId === endpointId)
    return device?.deviceName || endpointId.slice(0, 16) + '...'
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

const onSyncNowAsync = async (ruleId: string) => {
  isSyncing.value = ruleId
  try {
    const result = await syncStore.triggerSyncNowAsync(ruleId)
    if (result) {
      add({
        title: t('toast.syncComplete'),
        description: `${result.filesDownloaded} ${t('toast.filesDownloaded')}`,
        color: 'success',
      })
    }
  } catch (error) {
    add({
      title: t('toast.syncFailed'),
      description: error instanceof Error ? error.message : String(error),
      color: 'error',
    })
  } finally {
    isSyncing.value = null
  }
}

const onToggleAsync = async (ruleId: string, enabled: boolean) => {
  try {
    await syncStore.toggleRuleAsync(ruleId, enabled)
  } catch (error) {
    add({
      title: t('error'),
      description: error instanceof Error ? error.message : String(error),
      color: 'error',
    })
  }
}

const onEdit = (rule: SelectHaexSyncRules) => {
  editingRule.value = rule
  showCreateDialog.value = true
}

const onRuleCreated = async () => {
  await syncStore.loadRulesAsync()
  await syncStore.refreshStatusAsync()
}

const onDeleteAsync = async (ruleId: string) => {
  try {
    await syncStore.deleteRuleAsync(ruleId)
    add({ title: t('toast.deleted'), color: 'neutral' })
  } catch (error) {
    add({
      title: t('error'),
      description: error instanceof Error ? error.message : String(error),
      color: 'error',
    })
  }
}
</script>

<i18n lang="yaml">
de:
  title: Sync-Regeln
  description: Dateien automatisch zwischen Geräten und Cloud-Speicher synchronisieren
  addRule: Neue Regel
  empty: Noch keine Sync-Regeln erstellt
  error: Fehler
  label:
    source: Quelle
    target: Ziel
  direction:
    oneWay: Einseitig
    twoWay: Beidseitig
  status:
    running: Aktiv
    stopped: Inaktiv
    paused: Pausiert
    autoPaused: Auto-pausiert
    autoPausedTitle: Wegen wiederholter Fehler automatisch deaktiviert
  actions:
    viewLog: Aktivitäts-Log anzeigen
  log:
    title: Aktivitäts-Log
    clear: Löschen
    repeats: Wiederholungen
    empty: Noch keine Log-Einträge
    allDevices: Alle Geräte
    justNow: gerade eben
    minutesAgo: vor {n} min
    hoursAgo: vor {n} h
  intervals:
    manual: Nur manuell
  deleteModes:
    trash: Papierkorb
    permanent: Endgültig
    ignore: Ignorieren
  progress:
    preparing: Wird vorbereitet...
    files: Dateien
    active: aktiv
    done: fertig
    noData: Noch kein Sync durchgeführt
    moreFiles: weitere
    calculating: Berechne...
    finalizing: Abschließen…
  lastSync:
    title: Letzter Sync
    downloaded: heruntergeladen
    deleted: gelöscht
    upToDate: Alles aktuell
    moreErrors: weitere Fehler
  connection:
    direct: Direkt
    directTitle: Direkte LAN/WAN-Verbindung — voller Durchsatz
    relay: Relay
    relayTitle: Verbindung läuft über den Relay-Server — meist ~1 MB/s pro Stream
    unknown: Verbindung?
    unknownTitle: Noch keine aktive Verbindung — Diagnose nach erstem Sync verfügbar
    closed: Getrennt
    closedTitle: Verbindung wurde geschlossen
  toast:
    syncComplete: Sync abgeschlossen
    filesDownloaded: Dateien synchronisiert
    syncFailed: Sync fehlgeschlagen
    deleted: Regel gelöscht
en:
  title: Sync Rules
  description: Automatically synchronize files between devices and cloud storage
  addRule: New Rule
  empty: No sync rules created yet
  error: Error
  label:
    source: Source
    target: Target
  direction:
    oneWay: One-way
    twoWay: Two-way
  status:
    running: Active
    stopped: Inactive
    paused: Paused
    autoPaused: Auto-paused
    autoPausedTitle: Disabled automatically after repeated failures
  actions:
    viewLog: Show activity log
  log:
    title: Activity Log
    clear: Clear
    repeats: Repeats
    empty: No log entries yet
    allDevices: All devices
    justNow: just now
    minutesAgo: "{n} min ago"
    hoursAgo: "{n} h ago"
  intervals:
    manual: Manual only
  deleteModes:
    trash: Trash
    permanent: Permanent
    ignore: Ignore
  progress:
    preparing: Preparing...
    files: files
    active: active
    done: done
    noData: No sync has run yet
    moreFiles: more
    calculating: Calculating...
    finalizing: Finalizing…
  lastSync:
    title: Last sync
    downloaded: downloaded
    deleted: deleted
    upToDate: Everything up to date
    moreErrors: more errors
  connection:
    direct: Direct
    directTitle: Direct LAN/WAN connection — full throughput
    relay: Relay
    relayTitle: Connection runs through the relay server — typically caps at ~1 MB/s per stream
    unknown: Connection?
    unknownTitle: No active connection yet — diagnostics available after the first sync
    closed: Closed
    closedTitle: Connection has been closed
  toast:
    syncComplete: Sync complete
    filesDownloaded: files synced
    syncFailed: Sync failed
    deleted: Rule deleted
</i18n>

