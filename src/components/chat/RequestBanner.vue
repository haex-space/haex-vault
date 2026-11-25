<script setup lang="ts">
/**
 * The answer to a request (spec 099 FR-006) in place of the composer: who knocked, when last, and
 * "Annehmen" or "Ablehnen". Nothing of the stranger's messages was kept (FR-002). Accepting opens
 * the sender's Visitenkarte in the address book's preview (spec 060); the conversation moves to the
 * list once the contact is stored. Declining blocks the vault and tells it, so it asks first.
 */
const props = defineProps<{ name: string; lastAt: number | null }>()

const emit = defineEmits<{
  accept: []
  decline: []
}>()

const { t, d } = useI18n()
const confirming = ref(false)

function confirm() {
  confirming.value = false
  emit('decline')
}
</script>

<template>
  <div class="space-y-3" data-testid="chat-request-banner">
    <p class="text-sm text-muted-foreground">
      {{ t('chat.request.banner', { name: props.name }) }}
    </p>
    <p
      v-if="props.lastAt !== null"
      class="text-xs text-muted-foreground"
      data-testid="chat-request-last"
    >
      {{
        t('chat.request.lastKnock', {
          at: d(new Date(props.lastAt), {
            dateStyle: 'medium',
            timeStyle: 'short',
          }),
        })
      }}
    </p>
    <div class="flex flex-wrap justify-end gap-2">
      <UiButton
        type="button"
        variant="outline"
        data-testid="chat-request-decline"
        @click="confirming = true"
      >
        {{ t('chat.request.decline') }}
      </UiButton>
      <UiButton
        type="button"
        data-testid="chat-request-accept"
        @click="emit('accept')"
      >
        {{ t('chat.request.accept') }}
      </UiButton>
    </div>

    <UiDrawerModal
      v-if="confirming"
      :open="confirming"
      :title="t('chat.request.declineTitle', { name: props.name })"
      @update:open="confirming = $event"
    >
      <template #content>
        <p class="px-6 py-2 text-sm">
          {{ t('chat.request.declineConfirm', { name: props.name }) }}
        </p>
      </template>
      <template #footer>
        <div class="flex justify-end gap-2">
          <UiButton type="button" variant="outline" @click="confirming = false">
            {{ t('chat.cancel') }}
          </UiButton>
          <UiButton
            type="button"
            variant="destructive"
            data-testid="chat-request-confirm"
            @click="confirm"
          >
            {{ t('chat.request.decline') }}
          </UiButton>
        </div>
      </template>
    </UiDrawerModal>
  </div>
</template>
