<script setup lang="ts">
/**
 * What an entry of the chat's list shows, as Signal lays it out (spec 072 FR-001): the picture, the
 * name with the time small at the top right, and under it the start of the last message over two
 * lines with its state (delivery mark or unread count) at the right. The entry's button, its test
 * id and its menu stay with the caller.
 */
defineProps<{
  name: string
  /** The start of the last message; its lines are already run together. */
  preview: string | null
  time: string
  /** The name stands out while there is something unread. */
  unread?: boolean
}>()
</script>

<template>
  <span class="flex min-w-0 flex-1 items-center gap-3">
    <slot name="avatar" />
    <span class="min-w-0 flex-1">
      <span class="flex items-baseline justify-between gap-2">
        <span class="flex min-w-0 items-center gap-1">
          <span
            class="truncate text-sm"
            :class="unread ? 'font-semibold' : 'font-medium'"
          >
            {{ name }}
          </span>
          <slot name="name-suffix" />
        </span>
        <span
          v-if="time"
          class="shrink-0 text-[0.7rem] font-normal tabular-nums text-muted-foreground"
        >
          {{ time }}
        </span>
      </span>
      <span class="mt-0.5 flex items-start justify-between gap-2">
        <span
          class="line-clamp-2 min-w-0 flex-1 break-words text-xs font-normal leading-4 text-muted-foreground"
        >
          {{ preview }}
        </span>
        <span class="flex h-4 shrink-0 items-center">
          <slot name="status" />
        </span>
      </span>
    </span>
  </span>
</template>
