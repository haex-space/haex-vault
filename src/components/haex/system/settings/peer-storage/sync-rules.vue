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
      <SyncRuleCard
        v-for="rule in syncStore.syncRules"
        :key="rule.id"
        :rule="rule"
        :is-syncing="isSyncing === rule.id"
        :cycle-key="state.cycleKey[rule.id] ?? 0"
        :expanded="!!state.expandedMap[rule.id]"
        :show-all-devices="!!state.showAllDevicesMap[rule.id]"
        @update:expanded="(val: boolean) => state.expandedMap[rule.id] = val"
        @sync-now="onSyncNowAsync(rule.id)"
        @edit="onEdit(rule)"
        @delete="onDeleteAsync(rule.id)"
        @toggle="(val: boolean) => onToggleAsync(rule.id, val)"
        @toggle-all-devices="(val: boolean) => state.onToggleAllDevicesAsync(rule.id, val)"
      />
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
import type { SelectHaexSyncRules } from '~/database/schemas'
import { useSyncRulesState, SyncRulesStateKey } from '@/composables/useSyncRulesState'
import SyncRuleCard from './sync-rules/SyncRuleCard.vue'

defineEmits<{ back: [] }>()

const { t } = useI18n()
const { add } = useToast()
const syncStore = useFileSyncStore()
const peerStorageStore = usePeerStorageStore()
const deviceStore = useDeviceStore()

const showCreateDialog = ref(false)
const editingRule = ref<SelectHaexSyncRules | null>(null)
const isSyncing = ref<string | null>(null)

// Composable hosts all shared state, watchers, formatters, and the connection
// diagnostics setInterval lifecycle. Provided via injection key so children
// (SyncRuleCard, SyncRuleProgress, SyncRuleActivityLog) read the same instance.
const state = useSyncRulesState()
provide(SyncRulesStateKey, state)

onMounted(async () => {
  // The badge in log entries resolves human-readable device names; without a
  // load the map is empty and we'd only ever fall back to the truncated id.
  deviceStore.loadKnownDevicesAsync().catch(() => { /* best effort */ })
  await syncStore.loadRulesAsync()
  await syncStore.refreshStatusAsync()
  await peerStorageStore.loadSpaceDevicesAsync()
})

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
    unavailable: Nicht erreichbar — wird automatisch wiederholt
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
    unavailable: Unreachable — retrying automatically
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
