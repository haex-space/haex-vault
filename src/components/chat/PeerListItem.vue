<script setup lang="ts">
/**
 * One contact or group conversation in the chat's list (spec 072 FR-001): picture or initials,
 * name, start of the last message, time, unread count and a muted mark.
 */
import type { PeerConversationEntry } from '~/composables/useConversations'
import { listTime } from '~/lib/chat/listTime'

const props = defineProps<{
  peer: PeerConversationEntry
  active: boolean
  /** The test hook; the "Ich" entry has its own. */
  testId?: string
}>()

const emit = defineEmits<{ select: [] }>()

const { t, d } = useI18n()

// The "Ich" entry before the first note has no time.
const time = computed(() =>
  props.peer.lastAt > 0 ? listTime(props.peer.lastAt, d) : '',
)
</script>

<template>
  <button
    type="button"
    :data-testid="testId ?? 'chat-peer'"
    class="flex w-full min-w-0 items-center gap-2 rounded-lg px-2 py-1.5 text-left text-sm transition-colors hover:bg-accent focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
    :class="{ 'bg-accent font-medium': active }"
    @click="emit('select')"
  >
    <ContactsAvatar :name="peer.displayName" :photo="peer.photo" />
    <span class="min-w-0 flex-1">
      <span class="flex items-center gap-1">
        <span class="truncate">{{ peer.displayName }}</span>
        <Icon
          v-if="peer.muted"
          name="lucide:bell-off"
          class="h-3 w-3 shrink-0 text-muted-foreground"
          :aria-label="t('chat.list.muted')"
        />
      </span>
      <span
        v-if="peer.lastText"
        class="block truncate text-xs font-normal text-muted-foreground"
      >
        {{ peer.lastText }}
      </span>
    </span>
    <span class="flex shrink-0 flex-col items-end gap-1">
      <span class="text-xs font-normal tabular-nums text-muted-foreground">{{
        time
      }}</span>
      <ShadcnBadge
        v-if="peer.unread > 0"
        class="h-4 min-w-4 justify-center px-1 text-[10px]"
        :aria-label="t('chat.list.unread', { count: peer.unread })"
      >
        {{ peer.unread }}
      </ShadcnBadge>
    </span>
  </button>
</template>
