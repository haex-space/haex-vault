<template>
  <HaexSystemSettingsLayout
    :title="t('config.title')"
    show-back
    @back="$emit('back')"
  >
    <!-- Server-Sync Section -->
    <div class="space-y-4">
      <h3 class="text-sm font-semibold text-highlighted uppercase tracking-wide">
        {{ t('config.serverSync.heading') }}
      </h3>

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
            <p class="text-sm text-muted mb-4">
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
              <span class="text-sm text-muted">{{
                t('config.units.seconds')
              }}</span>
            </div>
            <p class="text-xs text-muted mt-2">
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
            <p class="text-sm text-muted mb-4">
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
              <span class="text-sm text-muted">{{
                t('config.units.minutes')
              }}</span>
            </div>
            <p class="text-xs text-muted mt-2">
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
    </div>

    <!-- Divider -->
    <USeparator class="my-6" />

    <!-- P2P Network Section -->
    <div class="space-y-4">
      <h3 class="text-sm font-semibold text-highlighted uppercase tracking-wide">
        {{ t('config.p2p.heading') }}
      </h3>
      <p class="text-sm text-muted">
        {{ t('config.p2p.description') }}
      </p>

      <!-- Endpoint Status & ID -->
      <div v-if="peerStore.nodeId" class="flex items-center gap-2 text-sm">
        <span class="text-muted shrink-0">{{ t('config.p2p.endpointId') }}:</span>
        <code class="font-mono truncate text-highlighted">{{ peerStore.nodeId }}</code>
        <UButton
          icon="i-lucide-copy"
          color="neutral"
          variant="ghost"
          size="xs"
          @click="copyEndpointId"
        />
      </div>

      <!-- Start/Stop + Autostart -->
      <div class="flex items-center gap-4">
        <UiButton
          :icon="peerStore.running ? 'i-lucide-power-off' : 'i-lucide-power'"
          :color="peerStore.running ? 'error' : 'primary'"
          :loading="isToggling"
          @click="onToggleEndpointAsync"
        >
          {{ peerStore.running ? t('config.p2p.stop') : t('config.p2p.start') }}
        </UiButton>
        <UCheckbox
          v-model="autostart"
          :label="t('config.p2p.autostart')"
          @update:model-value="onToggleAutostartAsync"
        />
      </div>
    </div>
  </HaexSystemSettingsLayout>
</template>

<script setup lang="ts">
import { and, eq } from 'drizzle-orm'
import { haexVaultSettings } from '~/database/schemas'

defineEmits<{ back: [] }>()

const { t } = useI18n()
const { add } = useToast()
const { copy } = useClipboard()

const syncConfigStore = useSyncConfigStore()
const { config: syncConfig } = storeToRefs(syncConfigStore)
const peerStore = usePeerStorageStore()
const deviceStore = useDeviceStore()
const db = requireDb()

// --- Server-Sync config ---
const activeConfigTab = ref('continuous')
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
    await syncConfigStore.saveConfigAsync({
      continuousDebounceMs: Math.round(continuousDebounceSec.value * 1000),
    })
    add({ color: 'success', description: t('config.saveSuccess') })
  } catch (error) {
    console.error('Failed to save debounce setting:', error)
    add({ color: 'error', description: t('config.saveError') })
  }
}

const savePeriodicIntervalAsync = async () => {
  try {
    await syncConfigStore.saveConfigAsync({
      periodicIntervalMs: Math.round(periodicIntervalMin.value * 60000),
    })
    add({ color: 'success', description: t('config.saveSuccess') })
  } catch (error) {
    console.error('Failed to save interval setting:', error)
    add({ color: 'error', description: t('config.saveError') })
  }
}

// --- P2P Endpoint config ---
const isToggling = ref(false)
const autostart = ref(false)

const copyEndpointId = async () => {
  await copy(peerStore.nodeId)
  add({ title: t('config.p2p.toast.copied'), color: 'success' })
}

const onToggleAutostartAsync = async (value: boolean | 'indeterminate') => {
  if (value === 'indeterminate') return
  if (!db) return
  if (!deviceStore.deviceId) return

  try {
    const existing = await db.query.haexVaultSettings.findFirst({
      where: and(
        eq(haexVaultSettings.key, VaultSettingsKeyEnum.peerStorageAutostart),
        eq(haexVaultSettings.deviceId, deviceStore.deviceId),
      ),
    })

    if (existing) {
      await db
        .update(haexVaultSettings)
        .set({ value: value ? 'true' : 'false' })
        .where(eq(haexVaultSettings.id, existing.id))
    } else {
      await db.insert(haexVaultSettings).values({
        id: crypto.randomUUID(),
        key: VaultSettingsKeyEnum.peerStorageAutostart,
        deviceId: deviceStore.deviceId,
        value: value ? 'true' : 'false',
      })
    }
  } catch (error) {
    console.error('Failed to save autostart setting:', error)
    add({ description: t('config.saveError'), color: 'error' })
  }
}

const onToggleEndpointAsync = async () => {
  isToggling.value = true
  try {
    if (peerStore.running) {
      await peerStore.stopAsync()
      add({ title: t('config.p2p.toast.stopped'), color: 'neutral' })
    } else {
      await peerStore.startAsync()
      add({ title: t('config.p2p.toast.started'), color: 'success' })
    }
  } catch (error) {
    add({
      title: t('config.saveError'),
      description: error instanceof Error ? error.message : String(error),
      color: 'error',
    })
  } finally {
    isToggling.value = false
  }
}

onMounted(async () => {
  await peerStore.refreshStatusAsync()
  if (db && deviceStore.deviceId) {
    const row = await db.query.haexVaultSettings.findFirst({
      where: and(
        eq(haexVaultSettings.key, VaultSettingsKeyEnum.peerStorageAutostart),
        eq(haexVaultSettings.deviceId, deviceStore.deviceId),
      ),
    })
    autostart.value = row?.value !== 'false'
  }
})
</script>

<i18n lang="yaml">
de:
  config:
    title: Konfiguration
    save: Speichern
    saveSuccess: Einstellungen gespeichert
    saveError: Fehler beim Speichern der Einstellungen
    serverSync:
      heading: Server-Sync
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
    p2p:
      heading: P2P-Netzwerk
      description: Verschlüsselte Peer-to-Peer-Verbindung zu anderen Geräten
      endpointId: Endpoint-ID
      autostart: Automatisch starten wenn die Vault geöffnet wird
      start: Start
      stop: Stop
      toast:
        copied: Endpoint-ID kopiert
        started: P2P-Endpoint gestartet
        stopped: P2P-Endpoint gestoppt
en:
  config:
    title: Configuration
    save: Save
    saveSuccess: Settings saved
    saveError: Error saving settings
    serverSync:
      heading: Server Sync
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
    p2p:
      heading: P2P Network
      description: Encrypted peer-to-peer connection to other devices
      endpointId: Endpoint ID
      autostart: Automatically start when the vault is opened
      start: Start
      stop: Stop
      toast:
        copied: Endpoint ID copied
        started: P2P endpoint started
        stopped: P2P endpoint stopped
</i18n>
