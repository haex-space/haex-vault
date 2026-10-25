<script setup lang="ts">
/**
 * "Geräte-Dateizugriff" (spec 056 US5, FR-014): whether the other devices of this vault may open
 * this device's files. On by default, a setting of this device only, which no other device can turn
 * back on; shares to contacts work either way. Writes go through the catalog action, which no agent
 * may call (a guardrail).
 */
import { invoke } from '@tauri-apps/api/core'

const { t } = useI18n()
const { errString } = useErrorString()
const setAccess = useActionOrThrow('settings.files.deviceAccess.set')

const enabled = ref<boolean | null>(null)
const busy = ref(false)
const savedFlash = ref(false)
const loadError = ref<string | null>(null)
const opError = ref<string | null>(null)

async function reloadAsync() {
  try {
    enabled.value = await invoke<boolean>('files_device_access_get')
    loadError.value = null
  } catch (error: unknown) {
    loadError.value = errString(error)
  }
}

async function toggleAsync(next: boolean) {
  if (enabled.value === null || busy.value) return
  busy.value = true
  savedFlash.value = false
  opError.value = null
  try {
    const result = (await setAccess({ enabled: next })) as { enabled: boolean }
    enabled.value = result.enabled
    savedFlash.value = true
  } catch (error: unknown) {
    opError.value = errString(error)
    await reloadAsync()
  } finally {
    busy.value = false
  }
}

onMounted(reloadAsync)
</script>

<template>
  <section class="flex flex-col gap-2">
    <p v-if="loadError" class="text-sm text-destructive" role="alert">
      {{ t('errors.prefLoadFailed') }}: {{ loadError }}
    </p>

    <SettingsGroup v-if="enabled !== null">
      <SettingsRow
        :title="t('settings.federation.deviceAccess.title')"
        :description="t('settings.federation.deviceAccess.description')"
        label-for="file-device-access-switch"
      >
        <ShadcnSwitch
          id="file-device-access-switch"
          :model-value="enabled"
          :disabled="busy"
          data-testid="file-device-access-switch"
          @update:model-value="toggleAsync($event === true)"
        />
      </SettingsRow>
    </SettingsGroup>
    <p v-if="enabled !== null" class="px-1 text-xs text-muted-foreground">
      {{ t('settings.deviceOnly') }}
    </p>

    <p v-if="savedFlash" class="px-1 text-xs text-success" role="status">
      {{ t('settings.federation.deviceAccess.saved') }}
    </p>
    <p v-if="opError" class="px-1 text-xs text-destructive" role="alert">
      {{ t('settings.federation.deviceAccess.failed') }}: {{ opError }}
    </p>
  </section>
</template>
