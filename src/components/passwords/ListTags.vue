<script setup lang="ts">
/**
 * The tag chips of one list row. Only whole chips are shown: what does not fit into the room of the
 * row is cut off as a whole and counted in a "+N" chip (the full list is in its tooltip), so a chip
 * is never clipped in the middle of its name. A single chip that is wider than the room is
 * shortened with an ellipsis instead.
 *
 * How it works without measuring text: the chips sit in a wrapping row that is exactly one chip high
 * and clips what overflows; every chip that wrapped into the clipped second line is hidden, and
 * its number is counted after each layout change.
 */
import type { ItemHeader } from '@bindings/ItemHeader'

const props = defineProps<{
  tags: ItemHeader['tags']
}>()

const row = useTemplateRef<HTMLElement>('row')
const hidden = ref(0)

function measure() {
  const chips = row.value?.children
  if (!chips || chips.length === 0) {
    hidden.value = 0
    return
  }
  const firstLine = (chips[0] as HTMLElement).offsetTop
  let count = 0
  for (const chip of Array.from(chips) as HTMLElement[]) {
    if (chip.offsetTop > firstLine) count++
  }
  hidden.value = count
}

useResizeObserver(row, measure)
watch(
  () => props.tags,
  () => nextTick(measure),
  { flush: 'post' },
)
onMounted(measure)

const allNames = computed(() => props.tags.map((tag) => tag.name).join(', '))
</script>

<template>
  <span
    v-if="tags.length > 0"
    class="flex min-w-0 items-center justify-end gap-1.5"
    :title="hidden > 0 ? allNames : undefined"
    data-testid="passwords-list-tags"
  >
    <span
      ref="row"
      class="flex h-[1.375rem] min-w-0 flex-wrap content-start justify-end gap-x-1.5 overflow-hidden"
    >
      <ShadcnBadge
        v-for="tag in tags"
        :key="tag.id"
        variant="secondary"
        class="max-w-full"
        :style="tag.color ? { color: tag.color } : undefined"
      >
        <span class="min-w-0 truncate">{{ tag.name }}</span>
      </ShadcnBadge>
    </span>
    <ShadcnBadge
      v-if="hidden > 0"
      variant="outline"
      class="shrink-0"
      data-testid="passwords-list-tags-more"
    >
      +{{ hidden }}
    </ShadcnBadge>
  </span>
</template>
