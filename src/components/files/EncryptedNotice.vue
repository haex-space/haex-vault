<script setup lang="ts">
/**
 * The notice before the first encrypted folder on this device (spec 048 FR-002): what is inside
 * can only be read with this vault. Shown once; confirming goes on to naming the folder.
 */
const emit = defineEmits<{ confirm: [] }>()
const open = defineModel<boolean>('open', { required: true })

const { t } = useI18n()

function confirm() {
  open.value = false
  emit('confirm')
}
</script>

<template>
  <ShadcnAlertDialog
    :open="open"
    @update:open="(value: boolean) => (open = value)"
  >
    <ShadcnAlertDialogContent data-testid="files-encrypted-notice">
      <ShadcnAlertDialogHeader>
        <ShadcnAlertDialogTitle>{{
          t('files.encrypted.notice.title')
        }}</ShadcnAlertDialogTitle>
        <ShadcnAlertDialogDescription>
          {{ t('files.encrypted.notice.body') }}
        </ShadcnAlertDialogDescription>
      </ShadcnAlertDialogHeader>
      <ShadcnAlertDialogFooter>
        <ShadcnAlertDialogCancel>{{
          t('files.cancel')
        }}</ShadcnAlertDialogCancel>
        <UiButton data-testid="files-encrypted-notice-confirm" @click="confirm">
          {{ t('files.encrypted.notice.confirm') }}
        </UiButton>
      </ShadcnAlertDialogFooter>
    </ShadcnAlertDialogContent>
  </ShadcnAlertDialog>
</template>
