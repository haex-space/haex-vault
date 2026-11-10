<script setup lang="ts">
/**
 * The one-time notice before the voice assistant is first switched on on this device (spec 066
 * FR-037, T073): anyone who says the wake word can talk to haex and confirm approvals, secrets
 * included. Open while the assistant store waits for the answer (`notice`); confirming switches the
 * assistant on, cancelling or closing leaves it off.
 */
const { t } = useI18n()
const assistant = useAssistantStore()

const open = computed(() => assistant.notice !== null)
</script>

<template>
  <ShadcnAlertDialog
    :open="open"
    @update:open="(value: boolean) => value || assistant.answerNotice(false)"
  >
    <ShadcnAlertDialogContent data-testid="assistant-notice">
      <ShadcnAlertDialogHeader>
        <ShadcnAlertDialogTitle>{{
          t('assistant.notice.title')
        }}</ShadcnAlertDialogTitle>
        <ShadcnAlertDialogDescription>
          {{ t('assistant.notice.body') }}
        </ShadcnAlertDialogDescription>
      </ShadcnAlertDialogHeader>
      <ShadcnAlertDialogFooter>
        <ShadcnAlertDialogCancel data-testid="assistant-notice-cancel">{{
          t('assistant.notice.cancel')
        }}</ShadcnAlertDialogCancel>
        <UiButton
          type="button"
          data-testid="assistant-notice-confirm"
          @click="assistant.answerNotice(true)"
        >
          {{ t('assistant.notice.confirm') }}
        </UiButton>
      </ShadcnAlertDialogFooter>
    </ShadcnAlertDialogContent>
  </ShadcnAlertDialog>
</template>
