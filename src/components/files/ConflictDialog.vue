<script setup lang="ts">
/**
 * What a transfer waits on. A name it finds taken at its target (spec 044 FR-021): replace, keep
 * both (the new one gets a number) or skip, optionally for every further name of this transfer.
 * Or, before it starts, files of an encrypted folder that would lie unencrypted in an ordinary
 * folder of a cloud storage (spec 048 FR-019): go on or cancel. Closing the dialog cancels the
 * transfer.
 */
import type { TransferAnswer } from '@bindings/TransferAnswer'

const props = defineProps<{
  open: boolean
  name: string
  /** Set when files leave an encrypted folder: how many. */
  leaving?: number | null
}>()
const emit = defineEmits<{
  answer: [choice: TransferAnswer, forAll: boolean]
  cancel: []
}>()

const { t } = useI18n()
const forAll = ref(false)
const leaves = computed(() => props.leaving != null)
watch(
  () => props.open,
  (open) => {
    if (open) forAll.value = false
  },
)
</script>

<template>
  <ShadcnAlertDialog
    :open="open"
    @update:open="(value: boolean) => !value && emit('cancel')"
  >
    <ShadcnAlertDialogContent
      v-if="leaves"
      data-testid="files-leave-encryption"
    >
      <ShadcnAlertDialogHeader>
        <ShadcnAlertDialogTitle>{{
          t('files.conflict.leaving.title')
        }}</ShadcnAlertDialogTitle>
        <ShadcnAlertDialogDescription>
          {{
            t(
              'files.conflict.leaving.body',
              { count: leaving ?? 0 },
              leaving ?? 0,
            )
          }}
        </ShadcnAlertDialogDescription>
      </ShadcnAlertDialogHeader>
      <ShadcnAlertDialogFooter class="flex-wrap gap-2">
        <UiButton
          variant="outline"
          data-testid="files-leave-encryption-cancel"
          @click="emit('answer', 'cancel', false)"
        >
          {{ t('files.cancel') }}
        </UiButton>
        <UiButton
          data-testid="files-leave-encryption-proceed"
          @click="emit('answer', 'proceed', false)"
        >
          {{ t('files.conflict.leaving.proceed') }}
        </UiButton>
      </ShadcnAlertDialogFooter>
    </ShadcnAlertDialogContent>
    <ShadcnAlertDialogContent v-else data-testid="files-conflict">
      <ShadcnAlertDialogHeader>
        <ShadcnAlertDialogTitle>{{
          t('files.conflict.title')
        }}</ShadcnAlertDialogTitle>
        <ShadcnAlertDialogDescription class="break-all">
          {{ t('files.conflict.body', { name }) }}
        </ShadcnAlertDialogDescription>
      </ShadcnAlertDialogHeader>
      <label class="flex items-center gap-2 text-sm">
        <ShadcnCheckbox v-model="forAll" data-testid="files-conflict-for-all" />
        {{ t('files.conflict.forAll') }}
      </label>
      <ShadcnAlertDialogFooter class="flex-wrap gap-2">
        <ShadcnAlertDialogCancel data-testid="files-conflict-cancel">
          {{ t('files.cancel') }}
        </ShadcnAlertDialogCancel>
        <UiButton
          variant="outline"
          data-testid="files-conflict-skip"
          @click="emit('answer', 'skip', forAll)"
        >
          {{ t('files.conflict.skip') }}
        </UiButton>
        <UiButton
          variant="outline"
          data-testid="files-conflict-keep-both"
          @click="emit('answer', 'keepBoth', forAll)"
        >
          {{ t('files.conflict.keepBoth') }}
        </UiButton>
        <UiButton
          data-testid="files-conflict-replace"
          @click="emit('answer', 'replace', forAll)"
        >
          {{ t('files.conflict.replace') }}
        </UiButton>
      </ShadcnAlertDialogFooter>
    </ShadcnAlertDialogContent>
  </ShadcnAlertDialog>
</template>
