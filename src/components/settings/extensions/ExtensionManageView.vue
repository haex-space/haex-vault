<script setup lang="ts">
/**
 * Managing one installed extension (spec 017, US7, T092): the switch saves on flipping and holds
 * on every own device (FR-007, FR-039); a new version installs from a file through the install
 * dialog, which shows what is new; removing asks whether its data stays (FR-008).
 */
import { invoke } from '@tauri-apps/api/core'
import type { ExtensionSummary } from '@bindings/ExtensionSummary'

const props = defineProps<{ extension: ExtensionSummary }>()
const { t } = useI18n()
const { errString } = useErrorString()
const tabRouter = useTabRouter()

const busy = ref(false)
const failure = ref<string | null>(null)
const updating = ref(false)
const removing = ref(false)

async function runAsync(action: () => Promise<unknown>): Promise<boolean> {
  if (busy.value) return false
  busy.value = true
  try {
    await action()
    failure.value = null
    return true
  } catch (error) {
    failure.value = errString(error)
    return false
  } finally {
    busy.value = false
  }
}

function setEnabledAsync(enabled: boolean) {
  return runAsync(() =>
    invoke('extension_set_enabled', {
      extensionId: props.extension.id,
      enabled,
    }),
  )
}

/** With its data kept the extension stays listed; deleted, the list is where to go on. */
function onRemoved(deleteData: boolean) {
  if (deleteData) tabRouter.replace('/extensions')
}
</script>

<template>
  <SettingsGroup :label="t('settings.extensions.manage')">
    <SettingsRow
      :title="t('settings.extensions.enabled')"
      :description="t('settings.extensions.enabledHint')"
      label-for="extension-enabled"
    >
      <ShadcnSwitch
        id="extension-enabled"
        :model-value="extension.enabled"
        :disabled="busy"
        data-testid="extension-enabled"
        @update:model-value="setEnabledAsync($event === true)"
      />
    </SettingsRow>
    <SettingsRow
      navigates
      icon="lucide:file-up"
      :title="t('settings.extensions.update')"
      :description="t('settings.extensions.updateHint')"
      data-testid="extension-update"
      @select="updating = true"
    />
    <SettingsRow
      :title="t('settings.extensions.removeTitle')"
      :description="t('settings.extensions.removeHint')"
    >
      <UiButton
        variant="destructive"
        size="sm"
        :disabled="busy"
        data-testid="extension-remove"
        @click="removing = true"
      >
        {{ t('settings.extensions.remove') }}
      </UiButton>
    </SettingsRow>
    <li v-if="failure" class="px-4 py-3 text-sm text-destructive" role="alert">
      {{ failure }}
    </li>
  </SettingsGroup>

  <ExtensionsInstallDialog v-model:open="updating" />

  <ExtensionsRemoveDialog
    v-model:open="removing"
    :extension="extension"
    @removed="onRemoved"
  />
</template>
