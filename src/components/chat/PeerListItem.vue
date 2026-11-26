<script setup lang="ts">
/**
 * One contact or group conversation in the chat's list (spec 072 FR-001): picture or initials,
 * name, two lines of the last message, how long ago it was sent, the delivery state of an own last
 * message or the unread count, and a muted mark. A contact or group can be muted from the context
 * menu (a long press on touch screens), as from the conversation's header.
 */
import type { PeerConversationEntry } from '~/composables/useConversations'

const props = defineProps<{
  peer: PeerConversationEntry
  active: boolean
  /** The test hook; the "Ich" entry has its own. */
  testId?: string
  /** Offers the mute switch; notes never notify and a request is answered first (FR-002b). */
  mutable?: boolean
}>()

const emit = defineEmits<{ select: []; mute: [muted: boolean] }>()

const { t } = useI18n()
const listTime = useListTime()

// The "Ich" entry before the first note has no time.
const time = computed(() =>
  props.peer.lastAt > 0 ? listTime(props.peer.lastAt) : '',
)
</script>

<template>
  <ShadcnContextMenu>
    <ShadcnContextMenuTrigger as-child :disabled="!mutable">
      <button
        type="button"
        :data-testid="testId ?? 'chat-peer'"
        class="flex w-full min-w-0 items-center rounded-lg px-3 py-3 text-left transition-colors hover:bg-accent focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
        :class="{ 'bg-accent': active }"
        @click="emit('select')"
      >
        <ChatListRowBody
          :name="peer.displayName"
          :preview="peer.lastText"
          :time="time"
          :unread="peer.unread > 0"
        >
          <template #avatar>
            <ContactsAvatar
              :name="peer.displayName"
              :photo="peer.photo"
              size="md"
            />
          </template>
          <template #name-suffix>
            <Icon
              v-if="peer.muted"
              data-testid="chat-peer-muted"
              name="lucide:bell-off"
              class="h-3 w-3 shrink-0 text-muted-foreground"
              :aria-label="t('chat.list.muted')"
            />
          </template>
          <template #status>
            <ShadcnBadge
              v-if="peer.unread > 0"
              class="h-4 min-w-4 justify-center px-1 text-[10px]"
              :aria-label="t('chat.list.unread', { count: peer.unread })"
            >
              {{ peer.unread }}
            </ShadcnBadge>
            <ChatDeliveryMark
              v-else-if="peer.lastDelivery"
              :state="peer.lastDelivery"
              class="h-3.5 w-3.5 text-muted-foreground"
              :class="{ 'text-destructive': peer.lastDelivery === 'failed' }"
              data-testid="chat-peer-list-delivery"
            />
          </template>
        </ChatListRowBody>
      </button>
    </ShadcnContextMenuTrigger>
    <ShadcnContextMenuContent v-if="mutable" class="min-w-48">
      <ShadcnContextMenuItem
        data-testid="chat-peer-menu-mute"
        @select="emit('mute', !peer.muted)"
      >
        {{ peer.muted ? t('chat.peer.unmute') : t('chat.peer.mute') }}
      </ShadcnContextMenuItem>
    </ShadcnContextMenuContent>
  </ShadcnContextMenu>
</template>
