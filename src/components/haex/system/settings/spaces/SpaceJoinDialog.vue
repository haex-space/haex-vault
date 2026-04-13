<template>
  <UiDrawerModal
    v-model:open="open"
    :title="t('title')"
    :description="t('description')"
  >
    <template #body>
      <UiInput
        v-model="inviteLink"
        :label="t('inviteLabel')"
        :placeholder="t('invitePlaceholder')"
        @keydown.enter.prevent="onSubmit"
      />
    </template>
    <template #footer>
      <div class="flex justify-between gap-4">
        <UButton
          color="neutral"
          variant="outline"
          @click="open = false"
        >
          {{ t('cancel') }}
        </UButton>
        <UiButton
          icon="i-lucide-log-in"
          :loading="submitting"
          :disabled="!inviteLink"
          @click="onSubmit"
        >
          {{ t('submit') }}
        </UiButton>
      </div>
    </template>
  </UiDrawerModal>
</template>

<script setup lang="ts">
const open = defineModel<boolean>('open', { required: true })

const props = defineProps<{
  initialInviteLink?: string
  submitting: boolean
}>()

const emit = defineEmits<{
  submit: [payload: { inviteLink: string }]
}>()

const { t } = useI18n()

const inviteLink = ref(props.initialInviteLink ?? '')

// Re-seed from parent when a new deeplink arrives and dialog opens.
watch(
  () => [open.value, props.initialInviteLink],
  ([isOpen, link]) => {
    if (isOpen) inviteLink.value = (link as string | undefined) ?? ''
  },
)

const onSubmit = () => {
  const trimmed = inviteLink.value.trim()
  if (!trimmed) return
  emit('submit', { inviteLink: trimmed })
}
</script>

<i18n lang="yaml">
de:
  title: Space beitreten
  description: Tritt einem Space mit einem Einladungslink bei
  inviteLabel: Einladungslink
  invitePlaceholder: haexvault://invite/...
  submit: Beitreten
  cancel: Abbrechen
en:
  title: Join Space
  description: Join a space using an invite link
  inviteLabel: Invite link
  invitePlaceholder: haexvault://invite/...
  submit: Join
  cancel: Cancel
</i18n>
