<template>
  <div>
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
          <div class="grid grid-cols-2 md:grid-cols-4 gap-4">
            <div class="bg-muted p-4 rounded-lg">
              <div class="text-sm text-muted">
                {{ t('overview.fileSize') }}
              </div>
              <div class="text-2xl font-bold">
                {{ dbInfo.fileSizeFormatted }}
              </div>
            </div>
            <div class="bg-muted p-4 rounded-lg">
              <div class="text-sm text-muted">
                {{ t('overview.totalEntries') }}
              </div>
              <div class="text-2xl font-bold">
                {{ dbInfo.totalEntries.toLocaleString() }}
              </div>
            </div>
            <div class="bg-muted p-4 rounded-lg">
              <div class="text-sm text-muted">
                {{ t('overview.activeEntries') }}
              </div>
              <div class="text-2xl font-bold text-success">
                {{ dbInfo.totalActive.toLocaleString() }}
              </div>
            </div>
            <div class="bg-muted p-4 rounded-lg">
              <div class="text-sm text-muted">
                {{ t('overview.tombstones') }}
              </div>
              <div class="text-2xl font-bold text-warning">
                {{ dbInfo.totalTombstones.toLocaleString() }}
              </div>
            </div>
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
            <template #default="{ item, open }">
              <UButton
                color="neutral"
                variant="ghost"
                class="w-full"
              >
                <div class="flex items-center justify-between w-full">
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
                    <UIcon
                      name="i-heroicons-chevron-down"
                      class="w-5 h-5 transition-transform"
                      :class="{ 'rotate-180': open }"
                    />
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
          <UFormField
            :label="t('retention.label')"
            :description="t('retention.description')"
          >
            <div class="flex items-center gap-2">
              <UiInput
                v-model.number="retentionDays"
                type="number"
                min="1"
                max="365"
                class="w-24"
              />
              <span class="text-muted">{{ t('retention.days') }}</span>
            </div>
          </UFormField>

          <div class="flex flex-col gap-3">
            <div class="flex items-center justify-between">
              <div>
                <div class="font-medium">
                  {{ t('actions.cleanup.label') }}
                </div>
                <div class="text-sm text-muted">
                  {{ t('actions.cleanup.description') }}
                </div>
              </div>
              <UButton
                :loading="isCleaningUp"
                :disabled="isCleaningUp || isVacuuming"
                @click="onCleanupAsync"
              >
                {{ t('actions.cleanup.button') }}
              </UButton>
            </div>

            <div class="flex items-center justify-between">
              <div>
                <div class="font-medium">
                  {{ t('actions.vacuum.label') }}
                </div>
                <div class="text-sm text-muted">
                  {{ t('actions.vacuum.description') }}
                </div>
              </div>
              <UButton
                :loading="isVacuuming"
                :disabled="isCleaningUp || isVacuuming"
                variant="outline"
                @click="onVacuumAsync"
              >
                {{ t('actions.vacuum.button') }}
              </UButton>
            </div>
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
          <div class="text-sm mt-2 space-y-1">
            <div>
              {{ t('cleanup.tombstonesDeleted') }}:
              {{ lastCleanupResult.tombstonesDeleted }}
            </div>
            <div>
              {{ t('cleanup.appliedDeleted') }}:
              {{ lastCleanupResult.appliedDeleted }}
            </div>
            <div class="font-semibold">
              {{ t('cleanup.totalDeleted') }}:
              {{ lastCleanupResult.totalDeleted }}
            </div>
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

const isLoading = ref(true)
const dbInfo = ref<DatabaseInfo | null>(null)
const retentionDays = ref(30)
const isCleaningUp = ref(false)
const isVacuuming = ref(false)
const lastCleanupResult = ref<CleanupResult | null>(null)

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
  } catch (error) {
    console.error('Failed to load database info:', error)
    add({ description: t('errors.loadFailed'), color: 'error' })
  } finally {
    isLoading.value = false
  }
}

const onCleanupAsync = async () => {
  isCleaningUp.value = true
  lastCleanupResult.value = null

  try {
    const result = await invoke<CleanupResult>('crdt_cleanup_tombstones', {
      retentionDays: retentionDays.value,
    })
    lastCleanupResult.value = result
    add({ description: t('cleanup.success'), color: 'success' })
    await loadDatabaseInfoAsync()
  } catch (error) {
    console.error('Cleanup failed:', error)
    add({ description: t('cleanup.error'), color: 'error' })
  } finally {
    isCleaningUp.value = false
  }
}

const onVacuumAsync = async () => {
  isVacuuming.value = true

  try {
    await invoke('database_vacuum')
    add({ description: t('vacuum.success'), color: 'success' })
    await loadDatabaseInfoAsync()
  } catch (error) {
    console.error('Vacuum failed:', error)
    add({ description: t('vacuum.error'), color: 'error' })
  } finally {
    isVacuuming.value = false
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
    totalEntries: Gesamteinträge
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
    label: Tombstone-Aufbewahrung
    description: Wie lange gelöschte Einträge aufbewahrt werden sollen (für Sync-Konsistenz)
    days: Tage
  actions:
    title: Datenbankoptimierung
    cleanup:
      label: Tombstones bereinigen
      description: Entfernt alte Löschmarkierungen und bereits synchronisierte Einträge
      button: Bereinigen
    vacuum:
      label: Datenbank komprimieren
      description: Optimiert die Datenbankdatei und gibt ungenutzten Speicherplatz frei
      button: Komprimieren
  cleanup:
    success: Bereinigung erfolgreich abgeschlossen
    error: Bereinigung fehlgeschlagen
    tombstonesDeleted: Tombstones gelöscht
    appliedDeleted: Angewendete Einträge gelöscht
    totalDeleted: Gesamt gelöscht
  vacuum:
    success: Datenbank erfolgreich komprimiert
    error: Komprimierung fehlgeschlagen
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
    label: Tombstone Retention
    description: How long to keep deleted entries (for sync consistency)
    days: days
  actions:
    title: Database Optimization
    cleanup:
      label: Cleanup Tombstones
      description: Remove old deletion markers and already synced entries
      button: Cleanup
    vacuum:
      label: Compact Database
      description: Optimize the database file and reclaim unused space
      button: Compact
  cleanup:
    success: Cleanup completed successfully
    error: Cleanup failed
    tombstonesDeleted: Tombstones deleted
    appliedDeleted: Applied entries deleted
    totalDeleted: Total deleted
  vacuum:
    success: Database compacted successfully
    error: Compaction failed
  errors:
    loadFailed: Could not load database information
</i18n>
