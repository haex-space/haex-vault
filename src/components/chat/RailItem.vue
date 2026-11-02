<script setup lang="ts">
/**
 * One conversation of the collapsed chat sidebar (spec 076 FR-005, FR-006): only its avatar, with
 * the name as accessible name and tooltip, a mark when it is the open one and the unread count (or,
 * for the requests entry, the number of requests) as a badge.
 */
const props = defineProps<{
  name: string
  /** Photo hash of a contact; unused for the agent and the requests entry. */
  photo?: string | null
  kind: 'agent' | 'peer' | 'requests'
  active: boolean
  unread: number
}>()

const emit = defineEmits<{ select: [] }>()

const { t } = useI18n()

const label = computed(() =>
  props.unread > 0
    ? `${props.name}, ${t('chat.list.unread', { count: props.unread })}`
    : props.name,
)
</script>

<template>
  <UiTooltip :tooltip="name">
    <button
      type="button"
      data-testid="chat-rail-item"
      :data-active="active || undefined"
      :data-unread="unread > 0 ? unread : undefined"
      class="relative flex size-10 shrink-0 items-center justify-center rounded-lg transition-colors hover:bg-accent focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
      :class="{ 'bg-accent': active }"
      :aria-label="label"
      :aria-current="active || undefined"
      @click="emit('select')"
    >
      <span
        v-if="kind === 'agent'"
        class="flex size-9 items-center justify-center rounded-full bg-foreground text-background"
        aria-hidden="true"
      >
        <Icon name="lucide:bot" class="h-4 w-4" />
      </span>
      <span
        v-else-if="kind === 'requests'"
        class="flex size-9 items-center justify-center rounded-full bg-muted text-muted-foreground"
        aria-hidden="true"
      >
        <Icon name="lucide:inbox" class="h-4 w-4" />
      </span>
      <ContactsAvatar v-else :name="name" :photo="photo ?? null" />
      <ShadcnBadge
        v-if="unread > 0"
        class="absolute -right-0.5 -top-0.5 h-4 min-w-4 justify-center px-1 text-[10px]"
        aria-hidden="true"
      >
        {{ unread }}
      </ShadcnBadge>
    </button>
  </UiTooltip>
</template>
