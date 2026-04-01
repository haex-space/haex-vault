<template>
  <HaexSystemSettingsLayout
    :title="t('title')"
    :description="t('description')"
    show-back
    @back="$emit('back')"
  >
    <div class="space-y-4">
      <div class="flex flex-col @sm:flex-row @sm:items-center gap-3">
        <label class="text-sm font-medium flex-1">{{ t('port') }}</label>
        <div class="flex items-center gap-2">
          <UInput
            v-model.number="bridgePort"
            type="number"
            :min="1024"
            :max="65535"
            class="w-28"
            :disabled="savingPort"
          />
          <UButton
            :label="t('apply')"
            :loading="savingPort"
            :disabled="bridgePort === currentPort || !isValidPort"
            @click="handleSavePort"
          />
        </div>
      </div>

      <div class="flex items-center gap-2 text-sm">
        <UIcon
          :name="bridgeRunning ? 'i-heroicons-check-circle' : 'i-heroicons-x-circle'"
          :class="bridgeRunning ? 'text-success' : 'text-error'"
          class="w-4 h-4"
        />
        <span v-if="bridgeRunning">
          {{ t('running', { port: currentPort }) }}
        </span>
        <span v-else>
          {{ t('stopped') }}
        </span>
      </div>
    </div>
  </HaexSystemSettingsLayout>
</template>

<script setup lang="ts">
import { invoke } from '@tauri-apps/api/core'
import { isDesktop } from '~/utils/platform'

defineEmits<{ back: [] }>()

const { t } = useI18n()
const { add } = useToast()
const vaultSettingsStore = useVaultSettingsStore()

const bridgePort = ref(19455)
const currentPort = ref(19455)
const bridgeRunning = ref(false)
const savingPort = ref(false)

const isValidPort = computed(() => {
  return bridgePort.value >= 1024 && bridgePort.value <= 65535
})

const loadBridgeStatus = async () => {
  if (!isDesktop()) return
  try {
    const [running, port, savedPort] = await Promise.all([
      invoke<boolean>('external_bridge_get_status'),
      invoke<number>('external_bridge_get_port'),
      vaultSettingsStore.getExternalBridgePortAsync(),
    ])
    bridgeRunning.value = running
    currentPort.value = port
    bridgePort.value = savedPort
  } catch (error) {
    console.error('Error loading bridge status:', error)
  }
}

const handleSavePort = async () => {
  if (!isValidPort.value || bridgePort.value === currentPort.value) return

  savingPort.value = true
  try {
    const newPort = await vaultSettingsStore.updateExternalBridgePortAsync(bridgePort.value)

    await invoke('external_bridge_stop')
    await invoke('external_bridge_start', { port: newPort })

    currentPort.value = newPort
    bridgeRunning.value = true

    add({ description: t('portChanged', { port: newPort }), color: 'success' })
  } catch (error) {
    console.error('Error changing bridge port:', error)
    add({ description: t('portChangeError'), color: 'error' })
  } finally {
    savingPort.value = false
  }
}

onMounted(async () => {
  await loadBridgeStatus()
})
</script>

<i18n lang="yaml">
de:
  title: Bridge-Konfiguration
  description: "Port für die WebSocket-Verbindung externer Clients (Standard: 19455)"
  port: Port
  apply: Anwenden
  running: 'Bridge läuft auf Port {port}'
  stopped: Bridge ist gestoppt
  portChanged: 'Port wurde auf {port} geändert'
  portChangeError: Fehler beim Ändern des Ports
en:
  title: Bridge Configuration
  description: "Port for WebSocket connections from external clients (default: 19455)"
  port: Port
  apply: Apply
  running: 'Bridge running on port {port}'
  stopped: Bridge is stopped
  portChanged: 'Port changed to {port}'
  portChangeError: Error changing port
</i18n>
