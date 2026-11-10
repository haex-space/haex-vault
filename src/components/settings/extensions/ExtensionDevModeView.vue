<script setup lang="ts">
/**
 * Developer mode in "Erweiterungen" (spec 017, US12, T096, FR-063): the switch for this device,
 * loading a project folder and the development versions loaded here with "Entladen" (listed also
 * while the mode is off, since a loaded one keeps its prefix until unloaded). Switching
 * reloads holzi's window, so its policy lets it frame development servers (research R16).
 */
import { invoke } from '@tauri-apps/api/core'
import { toast } from 'vue-sonner'
import type { DevModeState } from '@bindings/DevModeState'

const { t } = useI18n()
const { errString } = useErrorString()
const store = useExtensionsStore()

const enabled = ref(false)
const busy = ref(false)
const loading = ref(false)

const loaded = computed(() => store.list.filter((e) => e.dev))

async function readAsync() {
  try {
    enabled.value = (
      await invoke<DevModeState>('extension_dev_mode_get')
    ).enabled
  } catch (error) {
    toast.error(errString(error))
  }
}

async function switchAsync(next: boolean) {
  if (busy.value) return
  busy.value = true
  try {
    await invoke<DevModeState>('extension_dev_mode_set', { enabled: next })
    // The policy of a document holds from its loading: the new one comes with a reload.
    window.location.reload()
  } catch (error) {
    toast.error(errString(error))
    busy.value = false
  }
}

async function unloadAsync(extensionId: string) {
  if (busy.value) return
  busy.value = true
  try {
    await invoke('extension_dev_unload', { extensionId })
  } catch (error) {
    toast.error(errString(error))
  } finally {
    busy.value = false
  }
}

onMounted(readAsync)
</script>

<template>
  <SettingsGroup :label="t('settings.extensions.dev.title')">
    <SettingsRow
      :title="t('settings.extensions.dev.mode')"
      :description="t('settings.extensions.dev.modeHint')"
      label-for="extension-dev-mode"
    >
      <ShadcnSwitch
        id="extension-dev-mode"
        :model-value="enabled"
        :disabled="busy"
        data-testid="extension-dev-mode"
        @update:model-value="switchAsync($event === true)"
      />
    </SettingsRow>
    <SettingsRow
      v-if="enabled"
      navigates
      icon="lucide:folder-code"
      :title="t('settings.extensions.dev.load')"
      :description="t('settings.extensions.dev.loadHint')"
      data-testid="extension-dev-load"
      @select="loading = true"
    />
    <!-- Also while the mode is off: a loaded version still holds its prefix until unloaded. -->
    <SettingsRow
      v-for="extension in loaded"
      :key="extension.id"
      :title="extension.title"
      :description="extension.version ?? ''"
      :data-extension-id="extension.id"
      data-testid="extension-dev-row"
    >
      <UiButton
        size="sm"
        variant="outline"
        :disabled="busy"
        data-testid="extension-dev-unload"
        @click="unloadAsync(extension.id)"
      >
        {{ t('settings.extensions.dev.unload') }}
      </UiButton>
    </SettingsRow>
  </SettingsGroup>

  <ExtensionsInstallDialog v-model:open="loading" dev />
</template>
