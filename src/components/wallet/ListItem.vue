<script setup lang="ts">
/**
 * One pass in Wallet's list (spec 092, FR-001, FR-010): icon, title (or the placeholder text), the
 * kind and the start, a badge when it is over, and the tags. It holds a header only: never a code.
 */
import type { ItemHeader } from '@bindings/ItemHeader'
import { ITEMS_MIME, itemsPayload } from '~/lib/passwords/dnd'
import { DEFAULT_ENTRY_ICON } from '~/lib/passwords/icons'
import { displayTitle, isExpired, localDay } from '~/lib/passwords/format'
import { formatStart } from '~/lib/passwords/passDate'
import { passFallbackIcon, passKindOf } from '~/lib/passwords/passType'

const props = defineProps<{
  header: ItemHeader
  position?: number
  count?: number
}>()

const emit = defineEmits<{ open: [] }>()

const { t, locale } = useI18n()

const title = computed(() => displayTitle(props.header.title))
const expired = computed(() =>
  isExpired(props.header.expiresAt, localDay(new Date())),
)
const start = computed(() =>
  formatStart(
    props.header,
    Intl.DateTimeFormat().resolvedOptions().timeZone,
    locale.value,
  ),
)
/** A pass dragged onto a folder of the sidebar moves into it; only its id travels. */
function onDragStart(event: DragEvent) {
  event.dataTransfer?.setData(ITEMS_MIME, itemsPayload([props.header.id]))
  if (event.dataTransfer) event.dataTransfer.effectAllowed = 'move'
}

const kind = computed(() => t(`passwords.kinds.${passKindOf(props.header)}`))
</script>

<template>
  <li
    class="group flex items-center pr-2 hover:bg-foreground/5"
    :aria-posinset="position"
    :aria-setsize="count"
  >
    <button
      type="button"
      class="flex min-h-14 min-w-0 flex-1 items-center gap-4 py-3 pl-4 text-left focus-visible:outline-none focus-visible:ring-[3px] focus-visible:ring-ring/50 focus-visible:ring-inset"
      draggable="true"
      :data-testid="`wallet-pass-${header.id}`"
      @click="emit('open')"
      @dragstart="onDragStart"
    >
      <span
        class="flex size-9 shrink-0 items-center justify-center rounded-lg bg-background"
        :style="header.color ? { color: header.color } : undefined"
        aria-hidden="true"
      >
        <span class="size-5">
          <PasswordsEntryIcon
            :value="header.icon"
            :fallback="passFallbackIcon(header) ?? DEFAULT_ENTRY_ICON"
          />
        </span>
      </span>
      <span class="flex min-w-0 flex-1 flex-col">
        <span
          class="truncate"
          :class="title === null ? 'text-muted-foreground italic' : ''"
        >
          {{ title ?? t('passwords.untitled') }}
        </span>
        <span
          class="truncate text-sm text-muted-foreground"
          data-testid="wallet-pass-subtitle"
        >
          {{ start ? `${kind} · ${start.place}` : kind }}
        </span>
      </span>
      <span class="flex shrink-0 flex-wrap items-center justify-end gap-1.5">
        <ShadcnBadge v-if="expired" variant="destructive">
          {{ t('passwords.expired') }}
        </ShadcnBadge>
        <ShadcnBadge
          v-for="tag in header.tags"
          :key="tag.id"
          variant="secondary"
          :style="tag.color ? { color: tag.color } : undefined"
        >
          {{ tag.name }}
        </ShadcnBadge>
      </span>
    </button>
  </li>
</template>
