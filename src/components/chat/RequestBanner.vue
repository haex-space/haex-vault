<script setup lang="ts">
/**
 * The answer to a request (spec 072 US5, FR-041, FR-042) in place of the composer: who wrote, and
 * "Annehmen", "Ablehnen", "Blockieren". Accepting opens the sender's Visitenkarte in the address
 * book's preview (spec 060); the conversation moves to the list once the contact is stored.
 * Declining and blocking remove the conversation, so they ask first.
 */
const props = defineProps<{ name: string }>()

const emit = defineEmits<{
  accept: []
  decline: []
  block: []
}>()

const { t } = useI18n()
const confirming = ref<'decline' | 'block' | null>(null)

function confirm() {
  const answer = confirming.value
  confirming.value = null
  if (answer === 'decline') emit('decline')
  else if (answer === 'block') emit('block')
}
</script>

<template>
  <div class="space-y-3" data-testid="chat-request-banner">
    <p class="text-sm text-muted-foreground">
      {{ t('chat.request.banner', { name: props.name }) }}
    </p>
    <div class="flex flex-wrap justify-end gap-2">
      <UiButton
        type="button"
        variant="outline"
        data-testid="chat-request-block"
        @click="confirming = 'block'"
      >
        {{ t('chat.request.block') }}
      </UiButton>
      <UiButton
        type="button"
        variant="outline"
        data-testid="chat-request-decline"
        @click="confirming = 'decline'"
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
      :open="confirming !== null"
      :title="t(`chat.request.${confirming}Title`, { name: props.name })"
      @update:open="confirming = $event ? confirming : null"
    >
      <template #content>
        <p class="px-6 py-2 text-sm">
          {{ t(`chat.request.${confirming}Confirm`, { name: props.name }) }}
        </p>
      </template>
      <template #footer>
        <div class="flex justify-end gap-2">
          <UiButton type="button" variant="outline" @click="confirming = null">
            {{ t('chat.cancel') }}
          </UiButton>
          <UiButton
            type="button"
            variant="destructive"
            data-testid="chat-request-confirm"
            @click="confirm"
          >
            {{ t(`chat.request.${confirming}`) }}
          </UiButton>
        </div>
      </template>
    </UiDrawerModal>
  </div>
</template>
