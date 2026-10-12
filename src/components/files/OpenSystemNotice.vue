<script setup lang="ts">
/**
 * The notice before a file of an encrypted folder opens with the system's app (spec 048 FR-028):
 * holzi places a decrypted copy on this device, which the other app may keep; holzi removes its
 * copy when the vault closes. "Nicht mehr fragen" holds for this folder on this device.
 */
const emit = defineEmits<{ confirm: [dontAskAgain: boolean] }>()
const open = defineModel<boolean>('open', { required: true })

const { t } = useI18n()
const dontAskAgain = ref(false)
watch(open, (value) => {
  if (value) dontAskAgain.value = false
})

function confirm() {
  open.value = false
  emit('confirm', dontAskAgain.value)
}
</script>

<template>
  <ShadcnAlertDialog
    :open="open"
    @update:open="(value: boolean) => (open = value)"
  >
    <ShadcnAlertDialogContent data-testid="files-open-system-notice">
      <ShadcnAlertDialogHeader>
        <ShadcnAlertDialogTitle>{{
          t('files.encrypted.openSystem.title')
        }}</ShadcnAlertDialogTitle>
        <ShadcnAlertDialogDescription>
          {{ t('files.encrypted.openSystem.body') }}
        </ShadcnAlertDialogDescription>
      </ShadcnAlertDialogHeader>
      <label class="flex items-center gap-2 text-sm">
        <ShadcnCheckbox
          v-model="dontAskAgain"
          data-testid="files-open-system-dont-ask"
        />
        {{ t('files.encrypted.openSystem.dontAskAgain') }}
      </label>
      <ShadcnAlertDialogFooter>
        <ShadcnAlertDialogCancel>{{
          t('files.cancel')
        }}</ShadcnAlertDialogCancel>
        <UiButton
          type="button"
          data-testid="files-open-system-confirm"
          @click="confirm"
        >
          {{ t('files.encrypted.openSystem.confirm') }}
        </UiButton>
      </ShadcnAlertDialogFooter>
    </ShadcnAlertDialogContent>
  </ShadcnAlertDialog>
</template>
