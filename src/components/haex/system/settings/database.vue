<template>
  <div>
    <div class="p-6 border-b border-base-content/10">
      <h2 class="text-2xl font-bold">
        {{ t('title') }}
      </h2>
    </div>

    <div class="p-6 space-y-6">
      <!-- CRDT Statistics -->
      <div v-if="stats" class="space-y-4">
        <h3 class="text-lg font-semibold">{{ t('stats.title') }}</h3>
        <div class="grid grid-cols-2 gap-4">
          <div class="bg-muted p-4 rounded-lg">
            <div class="text-sm text-muted">{{ t('stats.totalEntries') }}</div>
            <div class="text-2xl font-bold">{{ stats.total_entries }}</div>
          </div>
          <div class="bg-muted p-4 rounded-lg">
            <div class="text-sm text-muted">{{ t('stats.pendingUpload') }}</div>
            <div class="text-2xl font-bold">{{ stats.pending_upload }}</div>
          </div>
          <div class="bg-muted p-4 rounded-lg">
            <div class="text-sm text-muted">{{ t('stats.pendingApply') }}</div>
            <div class="text-2xl font-bold">{{ stats.pending_apply }}</div>
          </div>
          <div class="bg-muted p-4 rounded-lg">
            <div class="text-sm text-muted">{{ t('stats.applied') }}</div>
            <div class="text-2xl font-bold">{{ stats.applied }}</div>
          </div>
        </div>
        <div class="grid grid-cols-3 gap-4">
          <div class="bg-green-500/10 p-3 rounded-lg">
            <div class="text-sm text-green-600">{{ t('stats.inserts') }}</div>
            <div class="text-xl font-bold text-green-700">
              {{ stats.insert_count }}
            </div>
          </div>
          <div class="bg-blue-500/10 p-3 rounded-lg">
            <div class="text-sm text-blue-600">{{ t('stats.updates') }}</div>
            <div class="text-xl font-bold text-blue-700">
              {{ stats.update_count }}
            </div>
          </div>
          <div class="bg-red-500/10 p-3 rounded-lg">
            <div class="text-sm text-red-600">{{ t('stats.deletes') }}</div>
            <div class="text-xl font-bold text-red-700">
              {{ stats.delete_count }}
            </div>
          </div>
        </div>
      </div>

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

      <!-- Cleanup Actions -->
      <div class="space-y-4">
        <h3 class="text-lg font-semibold">{{ t('actions.title') }}</h3>

        <div class="flex flex-col gap-3">
          <div class="flex items-center justify-between">
            <div>
              <div class="font-medium">{{ t('actions.cleanup.label') }}</div>
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
              <div class="font-medium">{{ t('actions.vacuum.label') }}</div>
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
      <div v-if="lastCleanupResult" class="mt-4 p-4 bg-success/10 rounded-lg">
        <div class="font-medium text-success">{{ t('cleanup.success') }}</div>
        <div class="text-sm mt-2 space-y-1">
          <div>
            {{ t('cleanup.tombstonesDeleted') }}:
            {{ lastCleanupResult.tombstones_deleted }}
          </div>
          <div>
            {{ t('cleanup.appliedDeleted') }}:
            {{ lastCleanupResult.applied_deleted }}
          </div>
          <div class="font-semibold">
            {{ t('cleanup.totalDeleted') }}:
            {{ lastCleanupResult.total_deleted }}
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { invoke } from '@tauri-apps/api/core'
import type { CleanupResult } from '~~/src-tauri/bindings/CleanupResult'
import type { CrdtStats } from '~~/src-tauri/bindings/CrdtStats'

const { t } = useI18n()
const { add } = useToast()

const stats = ref<CrdtStats | null>(null)
const retentionDays = ref(30)
const isCleaningUp = ref(false)
const isVacuuming = ref(false)
const lastCleanupResult = ref<CleanupResult | null>(null)

const loadStatsAsync = async () => {
  try {
    stats.value = await invoke<CrdtStats>('crdt_get_stats')
  } catch (error) {
    console.error('Failed to load CRDT stats:', error)
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
    await loadStatsAsync()
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
  } catch (error) {
    console.error('Vacuum failed:', error)
    add({ description: t('vacuum.error'), color: 'error' })
  } finally {
    isVacuuming.value = false
  }
}

onMounted(async () => {
  await loadStatsAsync()
})
</script>

<i18n lang="yaml">
de:
  title: Datenbank
  stats:
    title: CRDT Statistiken
    totalEntries: Gesamteinträge
    pendingUpload: Warten auf Upload
    pendingApply: Warten auf Anwendung
    applied: Angewendet
    inserts: Einfügungen
    updates: Änderungen
    deletes: Löschungen
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
en:
  title: Database
  stats:
    title: CRDT Statistics
    totalEntries: Total Entries
    pendingUpload: Pending Upload
    pendingApply: Pending Apply
    applied: Applied
    inserts: Inserts
    updates: Updates
    deletes: Deletes
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
</i18n>
