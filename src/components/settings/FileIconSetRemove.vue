<script setup lang="ts">
/**
 * Removes a custom icon set after a confirmation (spec 053 FR-019): the delete reaches every device
 * of the vault, and a vault that had chosen the set falls back to the default.
 */
import { invoke } from '@tauri-apps/api/core'

const props = defineProps<{ id: string; name: string }>()
const emit = defineEmits<{ removed: []; failed: [message: string] }>()

const { t } = useI18n()
const { errString } = useErrorString()
const open = ref(false)
const busy = ref(false)

async function removeAsync() {
  busy.value = true
  try {
    await invoke('file_icon_set_remove', { id: props.id })
    open.value = false
    emit('removed')
  } catch (error) {
    open.value = false
    emit('failed', errString(error))
  } finally {
    busy.value = false
  }
}
</script>

<template>
  <ShadcnAlertDialog v-model:open="open">
    <UiButton
      variant="ghost"
      size="sm"
      :aria-label="t('settings.iconSet.remove.label', { name })"
      data-testid="settings-icon-set-remove"
      @click="open = true"
    >
      <Icon name="lucide:trash-2" class="size-4" />
    </UiButton>
    <ShadcnAlertDialogContent>
      <ShadcnAlertDialogHeader>
        <ShadcnAlertDialogTitle>{{
          t('settings.iconSet.remove.title', { name })
        }}</ShadcnAlertDialogTitle>
        <ShadcnAlertDialogDescription>{{
          t('settings.iconSet.remove.body')
        }}</ShadcnAlertDialogDescription>
      </ShadcnAlertDialogHeader>
      <ShadcnAlertDialogFooter>
        <ShadcnAlertDialogCancel>{{
          t('settings.iconSet.import.cancel')
        }}</ShadcnAlertDialogCancel>
        <UiButton
          variant="destructive"
          :loading="busy"
          data-testid="settings-icon-set-remove-confirm"
          @click="removeAsync"
        >
          {{ t('settings.iconSet.remove.confirm') }}
        </UiButton>
      </ShadcnAlertDialogFooter>
    </ShadcnAlertDialogContent>
  </ShadcnAlertDialog>
</template>
