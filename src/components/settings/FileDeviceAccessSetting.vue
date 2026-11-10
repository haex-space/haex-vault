<script setup lang="ts">
/**
 * "Geräte-Dateizugriff" (spec 056 US5, FR-014): whether the other devices of this vault may open
 * this device's files. On by default, a setting of this device only, which no other device can turn
 * back on; shares to contacts work either way. Writes go through the catalog action, which no agent
 * may call (a guardrail).
 */
import { toast } from 'vue-sonner'
import { invoke } from '@tauri-apps/api/core'

const { t } = useI18n()
const { errString } = useErrorString()
const setAccess = useActionOrThrow('settings.files.deviceAccess.set')

const enabled = ref<boolean | null>(null)
const busy = ref(false)

async function reloadAsync() {
  try {
    enabled.value = await invoke<boolean>('files_device_access_get')
  } catch (error: unknown) {
    toast.error(`${t('errors.prefLoadFailed')}: ${errString(error)}`)
  }
}

async function toggleAsync(next: boolean) {
  if (enabled.value === null || busy.value) return
  busy.value = true
  try {
    const result = (await setAccess({ enabled: next })) as { enabled: boolean }
    enabled.value = result.enabled
  } catch (error: unknown) {
    toast.error(
      `${t('settings.federation.deviceAccess.failed')}: ${errString(error)}`,
    )
    await reloadAsync()
  } finally {
    busy.value = false
  }
}

onMounted(reloadAsync)
</script>

<template>
  <section class="flex flex-col gap-2">
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
  </section>
</template>
