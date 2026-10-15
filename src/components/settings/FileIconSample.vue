<script setup lang="ts">
/**
 * One sample icon of the preview of a set (spec 053, FR-012): drawn through the same machinery as
 * the file browser, for the set it is given and not only for the chosen one.
 */
import type { IconQuery } from '@bindings/IconQuery'
import type { IconSetRef } from '@bindings/IconSetRef'

const props = defineProps<{
  set: IconSetRef
  query: IconQuery
}>()

const icon = useFileIcon(
  () => props.query,
  () => props.set,
)
</script>

<template>
  <span class="block size-5 shrink-0">
    <img
      v-if="icon?.kind === 'image'"
      :src="icon.url"
      alt=""
      class="size-full object-contain"
      draggable="false"
    />
    <Icon
      v-else-if="icon?.kind === 'builtin'"
      :name="icon.name"
      class="size-full text-muted-foreground"
    />
  </span>
</template>
