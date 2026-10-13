<script setup lang="ts">
/**
 * The question before deleting for good (spec 044 FR-023, spec 048 FR-046): on Android, in a
 * storage and in or of an encrypted folder, where there is no trash, it names how many entries go
 * and how many foreign files go with encrypted folders (FR-048). On desktops deleting ordinary
 * files moves them to the trash and asks nothing.
 */
withDefaults(defineProps<{ count: number; foreign?: number }>(), {
  foreign: 0,
})
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
    <ShadcnAlertDialogContent>
      <ShadcnAlertDialogHeader>
        <ShadcnAlertDialogTitle>{{
          t('files.delete.title')
        }}</ShadcnAlertDialogTitle>
        <ShadcnAlertDialogDescription data-testid="files-delete-count">
          {{ t('files.delete.body', { count }, count) }}
        </ShadcnAlertDialogDescription>
        <ShadcnAlertDialogDescription
          v-if="foreign > 0"
          data-testid="files-delete-foreign"
        >
          {{ t('files.delete.foreign', { count: foreign }, foreign) }}
        </ShadcnAlertDialogDescription>
      </ShadcnAlertDialogHeader>
      <ShadcnAlertDialogFooter>
        <ShadcnAlertDialogCancel>{{
          t('files.cancel')
        }}</ShadcnAlertDialogCancel>
        <UiButton
          variant="destructive"
          data-testid="files-delete-confirm"
          @click="confirm"
        >
          {{ t('files.delete.confirm') }}
        </UiButton>
      </ShadcnAlertDialogFooter>
    </ShadcnAlertDialogContent>
  </ShadcnAlertDialog>
</template>
