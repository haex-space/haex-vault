<script setup lang="ts">
/**
 * The kept data of a removed extension (spec 017, US7-6, FR-008, T092): its size on this device
 * and deleting it on every own device. A later install of the same extension finds the data until
 * then.
 */
import { invoke } from '@tauri-apps/api/core'
import { toast } from 'vue-sonner'
import type { ExtensionSummary } from '@bindings/ExtensionSummary'
import { formatFileSize } from '~/lib/passwords/format'

const props = defineProps<{ extension: ExtensionSummary }>()
const { t } = useI18n()
const { errString } = useErrorString()
const tabRouter = useTabRouter()

const confirming = ref(false)
const busy = ref(false)

async function purgeAsync() {
  if (busy.value) return
  busy.value = true
  try {
    await invoke('extension_purge_kept_data', {
      extensionId: props.extension.id,
    })
    confirming.value = false
    tabRouter.replace('/extensions')
  } catch (error) {
    toast.error(errString(error))
  } finally {
    busy.value = false
  }
}
</script>

<template>
  <SettingsGroup :label="t('settings.extensions.keptData')">
    <SettingsRow
      :title="t('settings.extensions.keptDataSize')"
      :description="
        extension.keptDataBytes === undefined
          ? t('settings.extensions.keptDataSizeUnknown')
          : formatFileSize(extension.keptDataBytes)
      "
    >
      <UiButton
        variant="destructive"
        size="sm"
        :disabled="busy"
        data-testid="extension-purge-kept-data"
        @click="confirming = true"
      >
        {{ t('settings.extensions.deleteData') }}
      </UiButton>
    </SettingsRow>
  </SettingsGroup>

  <ShadcnAlertDialog v-model:open="confirming">
    <ShadcnAlertDialogContent>
      <ShadcnAlertDialogHeader>
        <ShadcnAlertDialogTitle>{{
          t('settings.extensions.purgeConfirmTitle', { name: extension.title })
        }}</ShadcnAlertDialogTitle>
        <ShadcnAlertDialogDescription>{{
          t('settings.extensions.purgeConfirmBody')
        }}</ShadcnAlertDialogDescription>
      </ShadcnAlertDialogHeader>
      <ShadcnAlertDialogFooter>
        <ShadcnAlertDialogCancel>{{
          t('settings.extensions.cancel')
        }}</ShadcnAlertDialogCancel>
        <UiButton
          variant="destructive"
          :loading="busy"
          data-testid="extension-purge-kept-data-confirm"
          @click="purgeAsync"
        >
          {{ t('settings.extensions.deleteData') }}
        </UiButton>
      </ShadcnAlertDialogFooter>
    </ShadcnAlertDialogContent>
  </ShadcnAlertDialog>
</template>
