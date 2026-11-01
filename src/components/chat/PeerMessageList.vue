<script setup lang="ts">
/**
 * The messages of a contact conversation (spec 072 US2, FR-021, FR-024): ordered by `sortAt`, the
 * other side's with name and picture, own ones with their delivery state. A failed message names
 * its reason and can be sent again. _Read_ only appears while read receipts are on (Rust leaves
 * it out otherwise, FR-024).
 */
import type { ChatPeerMessage } from '@bindings/ChatPeerMessage'
import type { ChatDeliveryState } from '@bindings/ChatDeliveryState'

const props = defineProps<{
  messages: ChatPeerMessage[]
  peerName: string
  peerPhoto: string | null
}>()

const emit = defineEmits<{
  resend: [messageId: string]
}>()

const { t, locale } = useI18n()

const STATE_ICONS: Record<ChatDeliveryState, string> = {
  waiting: 'lucide:clock',
  sent: 'lucide:check',
  delivered: 'lucide:check-check',
  read: 'lucide:check-check',
  failed: 'lucide:circle-alert',
}

const sorted = computed(() =>
  [...props.messages].sort((a, b) =>
    a.sortAt === b.sortAt ? a.id.localeCompare(b.id) : a.sortAt - b.sortAt,
  ),
)

function sameDay(a: Date, b: Date) {
  return a.toDateString() === b.toDateString()
}

function time(at: number) {
  const date = new Date(at)
  return sameDay(date, new Date())
    ? date.toLocaleTimeString(locale.value, {
        hour: '2-digit',
        minute: '2-digit',
      })
    : date.toLocaleString(locale.value, {
        dateStyle: 'short',
        timeStyle: 'short',
      })
}

function reason(code: string | null) {
  const key = `chat.delivery.reason.${code ?? 'unknown'}`
  return t(key)
}
</script>

<template>
  <ol class="flex flex-col gap-3" data-testid="chat-peer-messages">
    <li
      v-for="message in sorted"
      :key="message.id"
      class="flex items-end gap-2"
      :class="message.own ? 'justify-end' : 'justify-start'"
      :data-testid="message.own ? 'chat-peer-message-own' : 'chat-peer-message'"
      :data-delivery="message.delivery ?? undefined"
    >
      <ContactsAvatar
        v-if="!message.own"
        :name="peerName"
        :photo="peerPhoto"
        size="sm"
        class="shrink-0"
      />
      <div
        class="flex max-w-[75%] flex-col gap-1 rounded-2xl px-3 py-2"
        :class="
          message.own
            ? 'rounded-br-sm bg-primary text-primary-foreground'
            : 'rounded-bl-sm bg-muted'
        "
      >
        <span v-if="!message.own" class="text-xs font-medium opacity-80">
          {{ peerName }}
        </span>
        <p class="whitespace-pre-wrap break-words text-sm">
          {{ message.text }}
        </p>
        <span
          class="flex items-center justify-end gap-1 text-[0.7rem] opacity-70"
        >
          {{ time(message.sentAt) }}
          <Icon
            v-if="message.own && message.delivery"
            :name="STATE_ICONS[message.delivery]"
            class="h-3.5 w-3.5"
            :class="message.delivery === 'read' ? 'text-sky-300' : ''"
            :aria-label="t(`chat.delivery.state.${message.delivery}`)"
            :title="t(`chat.delivery.state.${message.delivery}`)"
            data-testid="chat-peer-delivery"
          />
        </span>
        <div
          v-if="message.own && message.delivery === 'failed'"
          class="flex flex-wrap items-center justify-end gap-2 text-xs"
        >
          <span>{{ reason(message.failedReason) }}</span>
          <UiButton
            type="button"
            size="sm"
            variant="secondary"
            data-testid="chat-peer-resend"
            @click="emit('resend', message.id)"
          >
            {{ t('chat.delivery.resend') }}
          </UiButton>
        </div>
      </div>
    </li>
  </ol>
</template>
