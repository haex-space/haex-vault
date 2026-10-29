<script setup lang="ts">
/**
 * The question before a drive is released while transfers on it still run (spec 064 FR-010,
 * contracts/ui.md): only after the yes are they cancelled and the drive unmounted or ejected.
 */
defineProps<{ count: number; name: string; eject: boolean }>()
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
        <ShadcnAlertDialogTitle>{{ name }}</ShadcnAlertDialogTitle>
        <ShadcnAlertDialogDescription data-testid="files-drive-release-body">
          {{
            t(
              'files.drives.transfersRunning',
              {
                count,
                verb: t(
                  eject ? 'files.drives.ejectVerb' : 'files.drives.unmountVerb',
                ),
              },
              count,
            )
          }}
        </ShadcnAlertDialogDescription>
      </ShadcnAlertDialogHeader>
      <ShadcnAlertDialogFooter>
        <ShadcnAlertDialogCancel data-testid="files-drive-release-cancel">{{
          t('files.cancel')
        }}</ShadcnAlertDialogCancel>
        <UiButton
          variant="destructive"
          data-testid="files-drive-release-confirm"
          @click="confirm"
        >
          {{ t(eject ? 'files.drives.eject' : 'files.drives.unmount') }}
        </UiButton>
      </ShadcnAlertDialogFooter>
    </ShadcnAlertDialogContent>
  </ShadcnAlertDialog>
</template>
