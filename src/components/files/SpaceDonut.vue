<script setup lang="ts">
/**
 * The space of a drive or a cloud storage in the sidebar of the file browser: a donut that shows
 * the free share green and the used share red, and a popover with the total, used and free bytes
 * on a click. Without a known total (some cloud storages) the ring stays neutral and the popover
 * names only what is used.
 */
import { formatSpace, usedShare } from '~/lib/files/space'

defineOptions({ inheritAttrs: false })

const props = defineProps<{
  /** The name of the drive or storage, for the accessible name of the button. */
  name: string
  used: number
  total: number | null
}>()

const { t, locale } = useI18n()

// A total of 0 says nothing about the share either: the ring stays neutral then too, instead of
// showing a drive that holds nothing as entirely free.
const shareKnown = computed(() => props.total !== null && props.total > 0)
// The ring is 100 units long (`pathLength`), so the used arc is its percentage.
const usedLength = computed(() =>
  shareKnown.value ? usedShare(props.used, props.total!) * 100 : 0,
)
</script>

<template>
  <ShadcnPopover>
    <ShadcnPopoverTrigger as-child>
      <button
        type="button"
        class="shrink-0 rounded p-1.5 hover:bg-accent"
        :aria-label="t('files.sidebar.spaceOf', { name })"
        v-bind="$attrs"
      >
        <svg viewBox="0 0 20 20" class="size-4 -rotate-90" aria-hidden="true">
          <circle
            cx="10"
            cy="10"
            r="8"
            fill="none"
            stroke-width="4"
            pathLength="100"
            :class="shareKnown ? 'stroke-success' : 'stroke-muted-foreground'"
          />
          <circle
            v-if="usedLength > 0"
            cx="10"
            cy="10"
            r="8"
            fill="none"
            stroke-width="4"
            pathLength="100"
            class="stroke-destructive"
            :stroke-dasharray="`${usedLength} 100`"
          />
        </svg>
      </button>
    </ShadcnPopoverTrigger>
    <ShadcnPopoverContent align="start" class="w-56 p-3 text-sm">
      <p class="truncate pb-2 font-medium">{{ name }}</p>
      <dl class="grid grid-cols-[auto_1fr] items-center gap-x-3 gap-y-1">
        <dt class="flex items-center gap-2 text-muted-foreground">
          <span class="size-2 rounded-full bg-destructive" />
          {{ t('files.sidebar.spaceUsed') }}
        </dt>
        <dd class="text-right tabular-nums">
          {{ formatSpace(used, locale) }}
        </dd>
        <template v-if="total !== null">
          <dt class="flex items-center gap-2 text-muted-foreground">
            <span class="size-2 rounded-full bg-success" />
            {{ t('files.sidebar.spaceFree') }}
          </dt>
          <dd class="text-right tabular-nums">
            {{ formatSpace(Math.max(total - used, 0), locale) }}
          </dd>
          <dt class="pl-4 text-muted-foreground">
            {{ t('files.sidebar.spaceTotal') }}
          </dt>
          <dd class="text-right tabular-nums">
            {{ formatSpace(total, locale) }}
          </dd>
        </template>
      </dl>
    </ShadcnPopoverContent>
  </ShadcnPopover>
</template>
