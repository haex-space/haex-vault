<script setup lang="ts">
/**
 * "Bildschirmschutz" (spec 043 FR-011a): while a vault is open, the system allows no screenshots or
 * recordings of the window and shows an empty preview among the recent apps. On by default, a
 * setting of this device only, saved and applied as soon as it flips. Writes go through the catalog
 * action, which no agent may call (a guardrail).
 */
import { toast } from 'vue-sonner'
import { invoke } from '@tauri-apps/api/core'

const { t } = useI18n()
const { errString } = useErrorString()
const setProtection = useActionOrThrow('settings.privacy.screenCapture.set')

const enabled = ref<boolean | null>(null)
const busy = ref(false)

async function reloadAsync() {
  try {
    enabled.value = await invoke<boolean>('screen_capture_protection_get')
  } catch (error: unknown) {
    toast.error(`${t('errors.prefLoadFailed')}: ${errString(error)}`)
  }
}

async function toggleAsync(next: boolean) {
  if (enabled.value === null || busy.value) return
  busy.value = true
  try {
    const result = (await setProtection({ enabled: next })) as {
      enabled: boolean
    }
    enabled.value = result.enabled
  } catch (error: unknown) {
    toast.error(`${t('settings.screenCapture.failed')}: ${errString(error)}`)
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
        :title="t('settings.screenCapture.title')"
        :description="t('settings.screenCapture.description')"
        label-for="screen-capture-switch"
      >
        <ShadcnSwitch
          id="screen-capture-switch"
          :model-value="enabled"
          :disabled="busy"
          data-testid="screen-capture-switch"
          @update:model-value="toggleAsync($event === true)"
        />
      </SettingsRow>
    </SettingsGroup>
    <p v-if="enabled !== null" class="px-1 text-xs text-muted-foreground">
      {{ t('settings.deviceOnly') }}
    </p>
  </section>
</template>
