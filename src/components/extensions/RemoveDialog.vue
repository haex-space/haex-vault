<script setup lang="ts">
/**
 * Removing an installed extension (spec 017, FR-008): asks whether its data stays and then calls
 * `extension_remove`. Shared by the extension's settings and the details in the marketplace app
 * (spec 069), so the question and its wording exist once.
 */
import { invoke } from '@tauri-apps/api/core'

const props = defineProps<{ extension: { id: string; title: string } }>()
const open = defineModel<boolean>('open', { required: true })
const emit = defineEmits<{ removed: [deleteData: boolean] }>()

const { t } = useI18n()
const { errString } = useErrorString()

const busy = ref(false)
const failure = ref<string | null>(null)

watch(open, (isOpen) => {
  if (isOpen) failure.value = null
})

async function removeAsync(deleteData: boolean) {
  if (busy.value) return
  busy.value = true
  try {
    await invoke('extension_remove', {
      extensionId: props.extension.id,
      deleteData,
    })
    failure.value = null
    open.value = false
    emit('removed', deleteData)
  } catch (error) {
    failure.value = errString(error)
  } finally {
    busy.value = false
  }
}
</script>

<template>
  <ShadcnAlertDialog v-model:open="open">
    <ShadcnAlertDialogContent>
      <ShadcnAlertDialogHeader>
        <ShadcnAlertDialogTitle>{{
          t('settings.extensions.removeConfirmTitle', {
            name: extension.title,
          })
        }}</ShadcnAlertDialogTitle>
        <ShadcnAlertDialogDescription>{{
          t('settings.extensions.removeConfirmBody')
        }}</ShadcnAlertDialogDescription>
      </ShadcnAlertDialogHeader>
      <p v-if="failure" class="text-sm text-destructive" role="alert">
        {{ failure }}
      </p>
      <ShadcnAlertDialogFooter>
        <ShadcnAlertDialogCancel>{{
          t('settings.extensions.cancel')
        }}</ShadcnAlertDialogCancel>
        <UiButton
          variant="outline"
          :loading="busy"
          data-testid="extension-remove-keep"
          @click="removeAsync(false)"
        >
          {{ t('settings.extensions.keepData') }}
        </UiButton>
        <UiButton
          variant="destructive"
          :loading="busy"
          data-testid="extension-remove-delete"
          @click="removeAsync(true)"
        >
          {{ t('settings.extensions.deleteData') }}
        </UiButton>
      </ShadcnAlertDialogFooter>
    </ShadcnAlertDialogContent>
  </ShadcnAlertDialog>
</template>
