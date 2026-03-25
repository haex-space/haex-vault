<template>
  <HaexSystemSettingsLayout
    :title="t('config.title')"
    show-back
    @back="$emit('back')"
  >
    <UTabs
      v-model="activeConfigTab"
      :items="configTabItems"
    >
      <template #content="{ item }">
        <!-- Continuous Sync Settings (Push) -->
        <div
          v-if="item.value === 'continuous'"
          class="pt-4"
        >
          <p class="text-sm text-gray-500 dark:text-gray-400 mb-4">
            {{ t('config.continuous.description') }}
          </p>
          <label class="block text-sm font-medium mb-2">
            {{ t('config.debounce.label') }}
          </label>
          <div class="flex items-center gap-3">
            <UInput
              v-model.number="continuousDebounceSec"
              type="number"
              :min="0.1"
              :max="30"
              :step="0.5"
              class="w-24"
            />
            <span class="text-sm text-gray-500">{{
              t('config.units.seconds')
            }}</span>
          </div>
          <p class="text-xs text-gray-500 dark:text-gray-400 mt-2">
            {{ t('config.debounce.hint') }}
          </p>
          <UButton
            v-if="
              continuousDebounceSec !== syncConfig.continuousDebounceMs / 1000
            "
            class="mt-2"
            @click="saveContinuousDebounceAsync"
          >
            {{ t('config.save') }}
          </UButton>
        </div>

        <!-- Periodic Sync Settings (Pull) -->
        <div
          v-if="item.value === 'periodic'"
          class="pt-4"
        >
          <p class="text-sm text-gray-500 dark:text-gray-400 mb-4">
            {{ t('config.periodic.description') }}
          </p>
          <label class="block text-sm font-medium mb-2">
            {{ t('config.interval.label') }}
          </label>
          <div class="flex items-center gap-3">
            <UInput
              v-model.number="periodicIntervalMin"
              type="number"
              :min="1"
              :max="60"
              :step="1"
              class="w-24"
            />
            <span class="text-sm text-gray-500">{{
              t('config.units.minutes')
            }}</span>
          </div>
          <p class="text-xs text-gray-500 dark:text-gray-400 mt-2">
            {{ t('config.interval.hint') }}
          </p>
          <UButton
            v-if="
              periodicIntervalMin !== syncConfig.periodicIntervalMs / 60000
            "
            class="mt-2"
            @click="savePeriodicIntervalAsync"
          >
            {{ t('config.save') }}
          </UButton>
        </div>
      </template>
    </UTabs>
  </HaexSystemSettingsLayout>
</template>

<script setup lang="ts">
defineEmits<{ back: [] }>()

const { t } = useI18n()
const { add } = useToast()

const syncConfigStore = useSyncConfigStore()
const { config: syncConfig } = storeToRefs(syncConfigStore)

// Sync configuration
const activeConfigTab = ref('continuous')
// UI uses seconds for debounce, minutes for interval - convert from ms
const continuousDebounceSec = ref(syncConfig.value.continuousDebounceMs / 1000)
const periodicIntervalMin = ref(syncConfig.value.periodicIntervalMs / 60000)

const configTabItems = computed(() => [
  {
    value: 'continuous',
    label: t('config.continuous.label'),
  },
  {
    value: 'periodic',
    label: t('config.periodic.label'),
  },
])

const saveContinuousDebounceAsync = async () => {
  try {
    // Convert seconds to milliseconds for storage
    await syncConfigStore.saveConfigAsync({
      continuousDebounceMs: Math.round(continuousDebounceSec.value * 1000),
    })
    add({
      color: 'success',
      description: t('config.saveSuccess'),
    })
  } catch (error) {
    console.error('Failed to save debounce setting:', error)
    add({
      color: 'error',
      description: t('config.saveError'),
    })
  }
}

const savePeriodicIntervalAsync = async () => {
  try {
    // Convert minutes to milliseconds for storage
    await syncConfigStore.saveConfigAsync({
      periodicIntervalMs: Math.round(periodicIntervalMin.value * 60000),
    })
    add({
      color: 'success',
      description: t('config.saveSuccess'),
    })
  } catch (error) {
    console.error('Failed to save interval setting:', error)
    add({
      color: 'error',
      description: t('config.saveError'),
    })
  }
}
</script>

<i18n lang="yaml">
de:
  config:
    title: Sync-Konfiguration
    continuous:
      label: Push
      description: Lokale Änderungen werden nach einer kurzen Verzögerung an den Server gesendet.
    periodic:
      label: Fallback-Pull
      description: Änderungen werden normalerweise in Echtzeit empfangen. Der periodische Pull holt verpasste Änderungen nach, falls die Verbindung kurzzeitig unterbrochen war.
    debounce:
      label: Verzögerung
      hint: Wartezeit nach der letzten Änderung, bevor gesendet wird
    interval:
      label: Abruf-Intervall
      hint: Zeitabstand zwischen automatischen Fallback-Abrufen
    units:
      seconds: Sekunden
      minutes: Minuten
    save: Speichern
    saveSuccess: Einstellungen gespeichert
    saveError: Fehler beim Speichern der Einstellungen
en:
  config:
    title: Sync Configuration
    continuous:
      label: Push
      description: Local changes are sent to the server after a short delay.
    periodic:
      label: Fallback Pull
      description: Changes are normally received in real-time. The periodic pull catches up on missed changes if the connection was briefly interrupted.
    debounce:
      label: Delay
      hint: Wait time after the last change before sending
    interval:
      label: Fetch Interval
      hint: Time between automatic fallback fetches
    units:
      seconds: seconds
      minutes: minutes
    save: Save
    saveSuccess: Settings saved
    saveError: Error saving settings
</i18n>
