<template>
  <div class="h-full">
    <!-- Table Browser View -->
    <HaexSystemSettingsTableBrowser
      v-if="browseTable"
      :table-name="browseTable"
      @back="browseTable = null"
    />

    <!-- Database Overview -->
    <HaexSystemSettingsLayout
      v-else
      :title="t('title')"
    >
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
          <div class="grid grid-cols-1 @sm:grid-cols-2 @2xl:grid-cols-4 gap-4">
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

        <!-- Extensions Stats -->
        <div class="space-y-4">
          <h3 class="text-lg font-semibold">
            {{ t('extensions.title') }}
          </h3>
          <div class="flex items-center gap-4 text-xs text-muted">
            <span class="flex items-center gap-1">
              <span class="w-2 h-2 rounded-full bg-success" />
              {{ t('extensions.active') }}
            </span>
            <span class="flex items-center gap-1">
              <span class="w-2 h-2 rounded-full bg-info" />
              {{ t('extensions.modified') }}
            </span>
            <span class="flex items-center gap-1">
              <span class="w-2 h-2 rounded-full bg-warning" />
              {{ t('extensions.deleted') }}
            </span>
          </div>
          <UAccordion
            :items="extensionItems"
            multiple
            :ui="{ header: 'w-full', trigger: 'w-full flex-1', label: 'w-full flex-1' }"
          >
            <template #default="{ item }">
              <div class="flex items-center justify-between flex-1 gap-4">
                <div class="flex items-center gap-2 min-w-0">
                  <HaexIcon
                    :name="
                      item.iconUrl ||
                      (item.extensionId
                        ? 'i-heroicons-puzzle-piece'
                        : 'i-heroicons-cog-6-tooth')
                    "
                    class="w-5 h-5 shrink-0"
                  />
                  <span class="font-medium truncate">{{ item.label }}</span>
                  <UBadge
                    color="neutral"
                    variant="subtle"
                    class="shrink-0"
                  >
                    {{ item.tableCount }} {{ t('extensions.tables') }}
                  </UBadge>
                </div>
                <div class="flex items-center gap-3 @md:gap-4 shrink-0">
                  <span class="text-sm text-success">
                    {{ item.activeRows.toLocaleString() }}
                    <span class="hidden @md:inline">{{ t('extensions.active') }}</span>
                  </span>
                  <span
                    v-if="item.modifiedRows > 0"
                    class="text-sm text-info"
                  >
                    {{ item.modifiedRows.toLocaleString() }}
                    <span class="hidden @md:inline">{{ t('extensions.modified') }}</span>
                  </span>
                  <span
                    v-if="item.tombstoneRows > 0"
                    class="text-sm text-warning"
                  >
                    {{ item.tombstoneRows.toLocaleString() }}
                    <span class="hidden @md:inline">{{ t('extensions.deleted') }}</span>
                  </span>
                </div>
              </div>
            </template>
            <template #content="{ item }">
              <div class="pl-7 pr-4 pb-4 space-y-2">
                <div
                  v-for="table in item.tables"
                  :key="table.name"
                  class="flex items-center justify-between py-2 border-b border-default last:border-0 gap-4 cursor-pointer hover:bg-muted/50 rounded px-2 -mx-2 transition-colors"
                  @click="openTableBrowser(table.name)"
                >
                  <span class="font-mono text-sm truncate min-w-0">{{
                    formatTableName(table.name)
                  }}</span>
                  <div class="flex items-center gap-3 shrink-0 text-sm">
                    <span class="text-success">{{ table.activeRows }}</span>
                    <span
                      v-if="table.modifiedRows > 0"
                      class="text-info"
                    >
                      {{ table.modifiedRows }}
                    </span>
                    <span
                      v-if="table.tombstoneRows > 0"
                      class="text-warning"
                    >
                      {{ table.tombstoneRows }}
                    </span>
                    <UIcon
                      name="i-lucide-chevron-right"
                      class="w-4 h-4 text-muted"
                    />
                  </div>
                </div>
              </div>
            </template>
          </UAccordion>
        </div>

        <!-- Database Actions -->
        <div class="space-y-4">
          <h3 class="text-lg font-semibold">
            {{ t('actions.title') }}
          </h3>

          <!-- Tombstone Retention Setting -->
          <div
            class="flex flex-col @sm:flex-row @sm:items-center @sm:justify-between gap-3"
          >
            <div class="flex-1 min-w-0">
              <div class="font-medium">
                {{ t('retention.label') }}
              </div>
              <div class="text-sm text-muted">
                {{ t('retention.description') }}
              </div>
            </div>
            <div class="flex items-center gap-2 shrink-0">
              <USelectMenu
                v-model="retentionDays"
                :items="retentionOptions"
                value-key="value"
                class="w-28"
              />
              <span class="text-muted whitespace-nowrap">{{
                t('retention.days')
              }}</span>
            </div>
          </div>

          <!-- Force delete all tombstones -->
          <div
            class="flex flex-col @sm:flex-row @sm:items-center @sm:justify-between gap-3"
          >
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
              :disabled="isForceDeleting || !hasTombstones"
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
            {{
              t('cleanup.tombstonesDeleted', {
                count: lastCleanupResult.tombstonesDeleted,
              })
            }}
          </div>
        </div>
      </template>
    </HaexSystemSettingsLayout>
  </div>
</template>

<script setup lang="ts">
import { invoke } from '@tauri-apps/api/core'
import type { CleanupResult } from '~~/src-tauri/bindings/CleanupResult'
import type { DatabaseInfo } from '~~/src-tauri/bindings/DatabaseInfo'

const { t } = useI18n()
const { add } = useToast()
const vaultSettingsStore = useVaultSettingsStore()
const extensionsStore = useExtensionsStore()

const isLoading = ref(true)
const dbInfo = ref<DatabaseInfo | null>(null)
const retentionDays = ref(30)
const isForceDeleting = ref(false)
const lastCleanupResult = ref<CleanupResult | null>(null)

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

// Create a map of table name -> pending rows for quick lookup
const pendingSyncMap = computed(() => {
  if (!dbInfo.value) return new Map<string, number>()
  return new Map(
    dbInfo.value.pendingSync.map((p) => [p.tableName, p.pendingRows]),
  )
})

const extensionItems = computed(() => {
  if (!dbInfo.value) return []

  return dbInfo.value.extensions.map((ext) => {
    // Find the extension in the store to get its icon
    const storeExtension = ext.extensionId
      ? extensionsStore.availableExtensions.find(
          (e) => e.id === ext.extensionId,
        )
      : null

    // Calculate modified rows for this extension (sum of pending sync rows for its tables)
    const tablesWithModified = ext.tables.map((table) => ({
      ...table,
      modifiedRows: pendingSyncMap.value.get(table.name) ?? 0,
    }))
    const modifiedRows = tablesWithModified.reduce(
      (sum, t) => sum + Number(t.modifiedRows),
      0,
    )

    return {
      label: ext.name,
      extensionId: ext.extensionId,
      iconUrl: storeExtension?.iconUrl,
      tableCount: ext.tables.length,
      activeRows: ext.activeRows,
      tombstoneRows: ext.tombstoneRows,
      modifiedRows,
      tables: tablesWithModified,
    }
  })
})

const browseTable = ref<string | null>(null)

const openTableBrowser = (tableName: string) => {
  browseTable.value = tableName
}

const formatTableName = (name: string): string => {
  // Remove extension prefix for readability
  // Format: {public_key}__{extension_name}__{table}
  const parts = name.split('__')
  if (parts.length >= 3) {
    return parts.slice(2).join('__')
  }
  return name
}

const loadDatabaseInfoAsync = async () => {
  isLoading.value = true
  try {
    dbInfo.value = await invoke<DatabaseInfo>('get_database_info')
    // Load persisted retention days
    const persistedRetentionDays =
      await vaultSettingsStore.getTombstoneRetentionDaysAsync()
    retentionDays.value = persistedRetentionDays
  } catch (error) {
    console.error('Failed to load database info:', error)
    add({ description: t('errors.loadFailed'), color: 'error' })
  } finally {
    isLoading.value = false
  }
}

watch(retentionDays, async (newValue) => {
  try {
    await vaultSettingsStore.updateTombstoneRetentionDaysAsync(newValue)
    add({ description: t('retention.saved'), color: 'success' })
  } catch (error) {
    console.error('Failed to save retention days:', error)
    add({ description: t('retention.saveError'), color: 'error' })
  }
})

const onForceDeleteAsync = async () => {
  isForceDeleting.value = true
  lastCleanupResult.value = null

  try {
    // Force delete ALL delete-log entries (retention = 0)
    const result = await invoke<CleanupResult>('crdt_cleanup_deleted_rows', {
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
  title: Vault
  overview:
    fileSize: Dateigröße
    totalEntries: Einträge insgesamt
    activeEntries: Aktive Einträge
    tombstones: Gelöschte Einträge
  extensions:
    title: Einträge nach Erweiterung
    tables: Tabellen
    active: aktiv
    modified: geändert
    deleted: gelöscht
  retention:
    label: Aufbewahrungszeit
    description: Gelöschte Einträge werden für diese Zeit aufbewahrt, damit alle Geräte die Löschung synchronisieren können.
    days: Tage
    saved: Aufbewahrungszeit gespeichert
    saveError: Fehler beim Speichern der Aufbewahrungszeit
  actions:
    title: Datenbankoptimierung
    forceDelete:
      label: Alle Löschmarkierungen sofort entfernen
      description: 'Achtung: Geräte die noch nicht synchronisiert haben, könnten die gelöschten Einträge beim nächsten Sync wieder in diese Vault zurückschreiben.'
      button: Sofort löschen
  cleanup:
    success: Bereinigung erfolgreich abgeschlossen
    error: Bereinigung fehlgeschlagen
    tombstonesDeleted: '{count} Löschmarkierungen entfernt'
  forceDelete:
    success: Alle Löschmarkierungen wurden entfernt
    error: Fehler beim Löschen der Löschmarkierungen
  browser:
    rows: Einträge
    empty: Keine Einträge
  errors:
    loadFailed: Datenbankinformationen konnten nicht geladen werden

en:
  title: Vault
  overview:
    fileSize: File Size
    totalEntries: Total Entries
    activeEntries: Active Entries
    tombstones: Deleted Entries
  extensions:
    title: Entries by Extension
    tables: tables
    active: active
    modified: modified
    deleted: deleted
  retention:
    label: Retention Period
    description: Deleted entries are kept for this time so all devices can sync the deletion.
    days: days
    saved: Retention period saved
    saveError: Failed to save retention period
  actions:
    title: Database Optimization
    forceDelete:
      label: Remove all deletion markers now
      description: 'Warning: Devices that have not synced yet may push the deleted entries back into this vault on their next sync.'
      button: Delete now
  cleanup:
    success: Cleanup completed successfully
    error: Cleanup failed
    tombstonesDeleted: '{count} deletion markers removed'
  forceDelete:
    success: All deletion markers have been removed
    error: Failed to delete deletion markers
  browser:
    rows: rows
    empty: No entries
  errors:
    loadFailed: Could not load database information
</i18n>
