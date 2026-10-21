<script setup lang="ts">
/**
 * "Browser-Erweiterungen" (spec 049, US1, US5, FR-001, FR-002, FR-037): turns the External Bridge
 * on for this device, sets its port and shows its state. Saved as soon as a value changes, without
 * a save button; the write goes through the guardrail action `settings.browserBridge.set`, which
 * no agent can call. The approved and blocked clients join with US5.
 */
import type { BridgeOverview } from '~/types/bindings/BridgeOverview'
import { invoke } from '@tauri-apps/api/core'

const { t } = useI18n()
const { errString } = useErrorString()
const { capabilities } = useDeviceCapabilities()
const bridgeStore = useBrowserBridgeStore()
const setBridge = useActionOrThrow('settings.browserBridge.set')

const overview = ref<BridgeOverview | null>(null)
const port = ref('')
const busy = ref(false)
const failure = ref<string | null>(null)

const available = computed(() => capabilities.value?.browserBridge !== false)
const portValid = computed(() => {
  const value = Number(port.value)
  return Number.isInteger(value) && value >= 1024 && value <= 65535
})

async function reloadAsync() {
  try {
    overview.value = await invoke<BridgeOverview>('browser_bridge_overview')
    if (!busy.value) port.value = String(overview.value.port)
    failure.value = null
  } catch (error: unknown) {
    failure.value = errString(error)
  }
}

async function saveAsync(enabled: boolean, nextPort: number) {
  if (busy.value) return
  busy.value = true
  failure.value = null
  try {
    overview.value = (await setBridge({
      enabled,
      port: nextPort,
    })) as BridgeOverview
    port.value = String(overview.value.port)
  } catch (error: unknown) {
    failure.value = errString(error)
    await reloadAsync()
  } finally {
    busy.value = false
  }
}

function savePort() {
  if (!overview.value || !portValid.value) return
  if (Number(port.value) === overview.value.port) return
  void saveAsync(overview.value.enabled, Number(port.value))
}

onMounted(reloadAsync)
watch(() => bridgeStore.changes, reloadAsync)
</script>

<template>
  <section class="flex flex-col gap-3">
    <SettingsNotOnThisDevice v-if="!available" />

    <SettingsGroup v-if="overview">
      <SettingsRow
        :title="t('browserBridge.settings.enabled')"
        :description="t('browserBridge.settings.enabledHint')"
        label-for="browser-bridge-switch"
      >
        <ShadcnSwitch
          id="browser-bridge-switch"
          :model-value="overview.enabled"
          :disabled="busy || !available"
          data-testid="browser-bridge-switch"
          @update:model-value="saveAsync($event === true, overview.port)"
        />
      </SettingsRow>
      <SettingsRow
        :title="t('browserBridge.settings.port')"
        :description="t('browserBridge.settings.portHint')"
        label-for="browser-bridge-port"
      >
        <UiInput
          id="browser-bridge-port"
          v-model="port"
          class="w-28"
          inputmode="numeric"
          :disabled="busy || !available"
          :label="t('browserBridge.settings.port')"
          data-testid="browser-bridge-port"
          @blur="savePort"
          @keydown.enter.prevent="savePort"
        />
      </SettingsRow>
      <SettingsRow
        :title="t('browserBridge.settings.state')"
        :description="t(`browserBridge.state.${overview.state}`)"
        data-testid="browser-bridge-state"
        :data-state="overview.state"
      />
    </SettingsGroup>

    <p
      v-if="overview?.state === 'portInUse'"
      class="rounded-xl bg-muted px-3 py-2 text-sm"
      role="status"
      data-testid="browser-bridge-port-in-use"
    >
      {{ t('browserBridge.settings.portInUseHint', { port: overview.port }) }}
    </p>
    <p
      v-if="port && !portValid"
      class="px-1 text-xs text-destructive"
      role="alert"
    >
      {{ t('browserBridge.settings.portInvalid') }}
    </p>
    <p v-if="failure" class="px-1 text-xs text-destructive" role="alert">
      {{ failure }}
    </p>
  </section>
</template>
