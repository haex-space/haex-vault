<template>
  <div class="@container">
    <div class="p-6 border-b border-default">
      <h2 class="text-2xl font-bold">
        {{ t('title') }}
      </h2>
    </div>

    <div class="p-6 space-y-8">
      <!-- Loading State -->
      <div
        v-if="isLoading"
        class="flex justify-center py-8"
      >
        <UIcon
          name="i-heroicons-arrow-path"
          class="w-8 h-8 animate-spin text-muted"
        />
      </div>

      <template v-else-if="dbInfo">
        <!-- Overview Stats -->
        <div class="space-y-4">
          <h3 class="text-lg font-semibold">
            {{ t('overview.title') }}
          </h3>
          <div class="grid grid-cols-1 @sm:grid-cols-2 @lg:grid-cols-4 gap-4">
            <UiStatCard
              :label="t('overview.fileSize')"
              :value="dbInfo.fileSizeFormatted"
            />
            <UiStatCard
              :label="t('overview.totalEntries')"
              :value="dbInfo.totalEntries"
            />
            <UiStatCard
              :label="t('overview.activeEntries')"
              :value="dbInfo.totalActive"
              color="success"
            />
            <UiStatCard
              :label="t('overview.tombstones')"
              :value="dbInfo.totalTombstones"
              color="warning"
            />
          </div>
        </div>

        <!-- Pending Sync -->
        <div
          v-if="dbInfo.pendingSync.length > 0"
          class="space-y-4"
        >
          <div class="flex items-center justify-between">
            <h3 class="text-lg font-semibold">
              {{ t('pendingSync.title') }}
            </h3>
            <UBadge
              color="warning"
              variant="subtle"
            >
              {{ dbInfo.totalPendingSync.toLocaleString() }} {{ t('pendingSync.entries') }}
            </UBadge>
          </div>
          <div class="bg-warning/10 border border-warning/20 rounded-lg p-4">
            <div class="flex items-start gap-3">
              <UIcon
                name="i-heroicons-arrow-path"
                class="w-5 h-5 text-warning mt-0.5"
              />
              <div class="flex-1">
                <p class="text-sm text-warning font-medium">
                  {{ t('pendingSync.description') }}
                </p>
                <div class="mt-2 space-y-1">
                  <div
                    v-for="sync in dbInfo.pendingSync"
                    :key="sync.tableName"
                    class="text-xs text-muted flex justify-between"
                  >
                    <span class="font-mono">{{ formatTableName(sync.tableName) }}</span>
                    <span>{{ sync.pendingRows }} {{ t('pendingSync.rows') }}</span>
                  </div>
                </div>
              </div>
            </div>
          </div>
        </div>

        <!-- Extensions Stats -->
        <div class="space-y-4">
          <h3 class="text-lg font-semibold">
            {{ t('extensions.title') }}
          </h3>
          <UAccordion
            :items="extensionItems"
            multiple
          >
            <template #default="{ item }">
              <UButton
                color="neutral"
                variant="ghost"
                class="w-full"
              >
                <div class="flex items-center justify-between w-full gap-4">
                  <div class="flex items-center gap-2">
                    <UIcon
                      :name="item.extensionId ? 'i-heroicons-puzzle-piece' : 'i-heroicons-cog-6-tooth'"
                      class="w-5 h-5"
                    />
                    <span class="font-medium">{{ item.label }}</span>
                    <UBadge
                      size="xs"
                      color="neutral"
                      variant="subtle"
                    >
                      {{ item.tableCount }} {{ t('extensions.tables') }}
                    </UBadge>
                  </div>
                  <div class="flex items-center gap-4">
                    <span class="text-sm text-muted">
                      {{ item.activeRows.toLocaleString() }} {{ t('extensions.active') }}
                    </span>
                    <span
                      v-if="item.tombstoneRows > 0"
                      class="text-sm text-warning"
                    >
                      {{ item.tombstoneRows.toLocaleString() }} {{ t('extensions.deleted') }}
                    </span>
                  </div>
                </div>
              </UButton>
            </template>
            <template #content="{ item }">
              <div class="pl-7 pr-4 pb-4 space-y-2">
                <div
                  v-for="table in item.tables"
                  :key="table.name"
                  class="flex items-center justify-between py-2 border-b border-default last:border-0"
                >
                  <span class="font-mono text-sm">{{ formatTableName(table.name) }}</span>
                  <div class="flex items-center gap-4 text-sm">
                    <span class="text-success">{{ table.activeRows }} {{ t('extensions.active') }}</span>
                    <span
                      v-if="table.tombstoneRows > 0"
                      class="text-warning"
                    >
                      {{ table.tombstoneRows }} {{ t('extensions.deleted') }}
                    </span>
                  </div>
                </div>
              </div>
            </template>
          </UAccordion>
        </div>

        <!-- Tombstones Overview -->
        <div
          v-if="dbInfo.tombstones.length > 0"
          class="space-y-4"
        >
          <div class="flex items-center justify-between">
            <h3 class="text-lg font-semibold">
              {{ t('tombstones.title') }}
            </h3>
            <UBadge
              color="warning"
              variant="subtle"
            >
              {{ dbInfo.totalTombstones.toLocaleString() }} {{ t('tombstones.total') }}
            </UBadge>
          </div>
          <div class="border border-default rounded-lg overflow-hidden">
            <table class="w-full text-sm">
              <thead class="bg-muted">
                <tr>
                  <th class="px-4 py-2 text-left font-medium">
                    {{ t('tombstones.table') }}
                  </th>
                  <th class="px-4 py-2 text-left font-medium">
                    {{ t('tombstones.primaryKey') }}
                  </th>
                  <th class="px-4 py-2 text-left font-medium">
                    {{ t('tombstones.deletedAt') }}
                  </th>
                </tr>
              </thead>
              <tbody>
                <tr
                  v-for="(tombstone, index) in dbInfo.tombstones.slice(0, 20)"
                  :key="index"
                  class="border-t border-default"
                >
                  <td class="px-4 py-2 font-mono text-xs">
                    {{ formatTableName(tombstone.tableName) }}
                  </td>
                  <td class="px-4 py-2 font-mono text-xs truncate max-w-48">
                    {{ formatPrimaryKey(tombstone.primaryKey) }}
                  </td>
                  <td class="px-4 py-2 text-muted text-xs">
                    {{ formatHlcTimestamp(tombstone.deletedAt) }}
                  </td>
                </tr>
              </tbody>
            </table>
            <div
              v-if="dbInfo.tombstones.length > 20"
              class="px-4 py-2 text-center text-sm text-muted bg-muted"
            >
              {{ t('tombstones.andMore', { count: dbInfo.totalTombstones - 20 }) }}
            </div>
          </div>
        </div>

        <!-- Database Actions -->
        <div class="space-y-4">
          <h3 class="text-lg font-semibold">
            {{ t('actions.title') }}
          </h3>

          <!-- Tombstone Retention Setting -->
          <div class="flex flex-col @sm:flex-row @sm:items-center @sm:justify-between gap-3">
            <div class="flex-1 min-w-0">
              <div class="font-medium">
                {{ t('retention.label') }}
              </div>
              <div class="text-sm text-muted">
                {{ t('retention.description') }}
              </div>
            </div>
            <div class="flex flex-col @sm:flex-row items-stretch @sm:items-center gap-2 shrink-0 w-full @sm:w-auto">
              <div class="flex items-center gap-2">
                <USelectMenu
                  v-model="retentionDays"
                  :items="retentionOptions"
                  value-key="value"
                  class="w-20"
                />
                <span class="text-muted whitespace-nowrap">{{ t('retention.days') }}</span>
              </div>
              <UButton
                :loading="isSavingRetention"
                :disabled="!hasUnsavedRetentionChanges || isSavingRetention"
                class="w-full @sm:w-28 justify-center"
                @click="onSaveRetentionAsync"
              >
                {{ t('retention.save') }}
              </UButton>
            </div>
          </div>

          <!-- Cleanup old tombstones -->
          <div class="flex flex-col @sm:flex-row @sm:items-center @sm:justify-between gap-3">
            <div class="flex-1 min-w-0">
              <div class="font-medium">
                {{ t('actions.cleanup.label') }}
              </div>
              <div class="text-sm text-muted">
                {{ t('actions.cleanup.description', { days: savedRetentionDays }) }}
              </div>
            </div>
            <UButton
              :loading="isCleaningUp"
              :disabled="isCleaningUp || isForceDeleting"
              class="w-full @sm:w-28 shrink-0 justify-center"
              @click="onCleanupAsync"
            >
              {{ t('actions.cleanup.button') }}
            </UButton>
          </div>

          <!-- Force delete all tombstones -->
          <div class="flex flex-col @sm:flex-row @sm:items-center @sm:justify-between gap-3">
            <div class="flex-1 min-w-0">
              <div class="font-medium">
                {{ t('actions.forceDelete.label') }}
              </div>
              <div class="text-sm text-muted">
                {{ t('actions.forceDelete.description') }}
              </div>
            </div>
            <UButton
              :loading="isForceDeleting"
              :disabled="isForceDeleting || isCleaningUp || !hasTombstones"
              color="error"
              variant="soft"
              class="w-full @sm:w-28 shrink-0 justify-center"
              @click="onForceDeleteAsync"
            >
              {{ t('actions.forceDelete.button') }}
            </UButton>
          </div>
        </div>

        <!-- Last Cleanup Result -->
        <div
          v-if="lastCleanupResult"
          class="p-4 bg-success/10 rounded-lg"
        >
          <div class="font-medium text-success">
            {{ t('cleanup.success') }}
          </div>
          <div class="text-sm mt-2">
            {{ t('cleanup.tombstonesDeleted', { count: lastCleanupResult.tombstonesDeleted }) }}
          </div>
        </div>
      </template>
    </div>
  </div>
</template>

<script setup lang="ts">
import { invoke } from '@tauri-apps/api/core'
import type { CleanupResult } from '~~/src-tauri/bindings/CleanupResult'
import type { DatabaseInfo } from '~~/src-tauri/bindings/DatabaseInfo'

const { t } = useI18n()
const { add } = useToast()
const vaultSettingsStore = useVaultSettingsStore()

const isLoading = ref(true)
const dbInfo = ref<DatabaseInfo | null>(null)
const retentionDays = ref(30)
const savedRetentionDays = ref(30)
const isCleaningUp = ref(false)
const isForceDeleting = ref(false)
const isSavingRetention = ref(false)
const lastCleanupResult = ref<CleanupResult | null>(null)

const hasUnsavedRetentionChanges = computed(() => retentionDays.value !== savedRetentionDays.value)
const hasTombstones = computed(() => (dbInfo.value?.totalTombstones ?? 0) > 0)

const retentionOptions = [
  { label: '7', value: 7 },
  { label: '14', value: 14 },
  { label: '30', value: 30 },
  { label: '60', value: 60 },
  { label: '90', value: 90 },
  { label: '180', value: 180 },
  { label: '365', value: 365 },
]

const extensionItems = computed(() => {
  if (!dbInfo.value) return []

  return dbInfo.value.extensions.map((ext) => ({
    label: ext.name,
    extensionId: ext.extensionId,
    tableCount: ext.tables.length,
    activeRows: ext.activeRows,
    tombstoneRows: ext.tombstoneRows,
    tables: ext.tables,
  }))
})

const formatTableName = (name: string): string => {
  // Remove extension prefix for readability
  // Format: {public_key}__{extension_name}__{table}
  const parts = name.split('__')
  if (parts.length >= 3) {
    return parts.slice(2).join('__')
  }
  return name
}

const formatPrimaryKey = (pkJson: string): string => {
  try {
    const pk = JSON.parse(pkJson)
    return Object.values(pk).join(', ')
  } catch {
    return pkJson
  }
}

const formatHlcTimestamp = (hlc: string): string => {
  try {
    // HLC format: "time/node_id" - extract time part (NTP64 nanoseconds)
    const timePart = hlc.split('/')[0]
    if (!timePart) return hlc

    // Convert NTP64 nanoseconds to milliseconds
    const nanos = BigInt(timePart)
    const millis = Number(nanos / BigInt(1_000_000))

    // Create date from milliseconds
    const date = new Date(millis)
    if (isNaN(date.getTime())) return hlc

    return date.toLocaleString()
  } catch {
    return hlc
  }
}

const loadDatabaseInfoAsync = async () => {
  isLoading.value = true
  try {
    dbInfo.value = await invoke<DatabaseInfo>('get_database_info')
    // Load persisted retention days
    const persistedRetentionDays = await vaultSettingsStore.getTombstoneRetentionDaysAsync()
    retentionDays.value = persistedRetentionDays
    savedRetentionDays.value = persistedRetentionDays
  } catch (error) {
    console.error('Failed to load database info:', error)
    add({ description: t('errors.loadFailed'), color: 'error' })
  } finally {
    isLoading.value = false
  }
}

const onSaveRetentionAsync = async () => {
  isSavingRetention.value = true
  try {
    await vaultSettingsStore.updateTombstoneRetentionDaysAsync(retentionDays.value)
    savedRetentionDays.value = retentionDays.value
    add({ description: t('retention.saved'), color: 'success' })
  } catch (error) {
    console.error('Failed to save retention days:', error)
    add({ description: t('retention.saveError'), color: 'error' })
  } finally {
    isSavingRetention.value = false
  }
}

const onCleanupAsync = async () => {
  isCleaningUp.value = true
  lastCleanupResult.value = null

  try {
    // 1. Cleanup tombstones older than retention period
    const result = await invoke<CleanupResult>('crdt_cleanup_tombstones', {
      retentionDays: savedRetentionDays.value,
    })
    lastCleanupResult.value = result

    // 2. Vacuum to reclaim disk space
    await invoke('database_vacuum')

    add({ description: t('cleanup.success'), color: 'success' })
    await loadDatabaseInfoAsync()
  } catch (error) {
    console.error('Cleanup failed:', error)
    add({ description: t('cleanup.error'), color: 'error' })
  } finally {
    isCleaningUp.value = false
  }
}

const onForceDeleteAsync = async () => {
  isForceDeleting.value = true
  lastCleanupResult.value = null

  try {
    // Force delete ALL tombstones (retention = 0)
    const result = await invoke<CleanupResult>('crdt_cleanup_tombstones', {
      retentionDays: 0,
    })
    lastCleanupResult.value = result

    // Vacuum to reclaim disk space
    await invoke('database_vacuum')

    add({ description: t('forceDelete.success'), color: 'success' })
    await loadDatabaseInfoAsync()
  } catch (error) {
    console.error('Force delete failed:', error)
    add({ description: t('forceDelete.error'), color: 'error' })
  } finally {
    isForceDeleting.value = false
  }
}

onMounted(async () => {
  await loadDatabaseInfoAsync()
})
</script>

<i18n lang="yaml">
de:
  title: Datenbank
  overview:
    title: Übersicht
    fileSize: Dateigröße
    totalEntries: Einträge insgesamt
    activeEntries: Aktive Einträge
    tombstones: Gelöschte Einträge
  pendingSync:
    title: Ausstehende Synchronisierung
    description: Diese Tabellen haben Änderungen, die noch nicht synchronisiert wurden.
    entries: Einträge
    rows: Zeilen
  extensions:
    title: Einträge nach Erweiterung
    tables: Tabellen
    active: aktiv
    deleted: gelöscht
  tombstones:
    title: Gelöschte Einträge (Tombstones)
    total: gesamt
    table: Tabelle
    primaryKey: Primärschlüssel
    deletedAt: Gelöscht am
    andMore: '... und {count} weitere'
  retention:
    label: Aufbewahrungszeit
    description: Gelöschte Einträge werden für diese Zeit aufbewahrt, damit alle Geräte die Löschung synchronisieren können.
    days: Tage
    save: Speichern
    saved: Aufbewahrungszeit gespeichert
    saveError: Fehler beim Speichern der Aufbewahrungszeit
  actions:
    title: Datenbankoptimierung
    cleanup:
      label: Alte Löschmarkierungen entfernen
      description: 'Entfernt Löschmarkierungen die älter als {days} Tage sind. Diese Löschungen wurden bereits an alle Geräte synchronisiert.'
      button: Bereinigen
    forceDelete:
      label: Alle Löschmarkierungen sofort entfernen
      description: 'Achtung: Geräte die noch nicht synchronisiert haben, werden diese Löschungen nicht mehr erhalten. Gelöschte Einträge könnten dort wieder auftauchen.'
      button: Sofort löschen
  cleanup:
    success: Bereinigung erfolgreich abgeschlossen
    error: Bereinigung fehlgeschlagen
    tombstonesDeleted: '{count} Löschmarkierungen entfernt'
  forceDelete:
    success: Alle Löschmarkierungen wurden entfernt
    error: Fehler beim Löschen der Löschmarkierungen
  errors:
    loadFailed: Datenbankinformationen konnten nicht geladen werden

en:
  title: Database
  overview:
    title: Overview
    fileSize: File Size
    totalEntries: Total Entries
    activeEntries: Active Entries
    tombstones: Deleted Entries
  pendingSync:
    title: Pending Synchronization
    description: These tables have changes that have not been synced yet.
    entries: entries
    rows: rows
  extensions:
    title: Entries by Extension
    tables: tables
    active: active
    deleted: deleted
  tombstones:
    title: Deleted Entries (Tombstones)
    total: total
    table: Table
    primaryKey: Primary Key
    deletedAt: Deleted At
    andMore: '... and {count} more'
  retention:
    label: Retention Period
    description: Deleted entries are kept for this time so all devices can sync the deletion.
    days: days
    save: Save
    saved: Retention period saved
    saveError: Failed to save retention period
  actions:
    title: Database Optimization
    cleanup:
      label: Remove old deletion markers
      description: 'Removes deletion markers older than {days} days. These deletions have already been synced to all devices.'
      button: Cleanup
    forceDelete:
      label: Remove all deletion markers now
      description: 'Warning: Devices that have not synced yet will not receive these deletions. Deleted entries may reappear on those devices.'
      button: Delete now
  cleanup:
    success: Cleanup completed successfully
    error: Cleanup failed
    tombstonesDeleted: '{count} deletion markers removed'
  forceDelete:
    success: All deletion markers have been removed
    error: Failed to delete deletion markers
  errors:
    loadFailed: Could not load database information
</i18n>
