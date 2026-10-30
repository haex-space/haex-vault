<script setup lang="ts">
/**
 * The icon of an entry (spec 069, FR-006): the host loads it and hands over a `data:` URL, so the
 * webview never contacts a marketplace's image server. While it loads, and when it cannot be
 * loaded, a neutral placeholder stands in.
 */
import { invoke } from '@tauri-apps/api/core'

const props = defineProps<{
  sourceId: string
  url?: string
  size?: 'md' | 'lg'
}>()

const src = ref<string | null>(null)

watch(
  () => [props.sourceId, props.url] as const,
  async ([sourceId, url]) => {
    src.value = null
    if (!url) return
    try {
      const loaded = await invoke<string>('marketplace_icon', { sourceId, url })
      if (props.sourceId === sourceId && props.url === url) src.value = loaded
    } catch {
      // The placeholder stays.
    }
  },
  { immediate: true },
)

const box = computed(() => (props.size === 'lg' ? 'size-16' : 'size-12'))
</script>

<template>
  <span
    :class="[
      'flex shrink-0 items-center justify-center overflow-hidden rounded-xl bg-muted',
      box,
    ]"
    aria-hidden="true"
  >
    <img v-if="src" :src="src" alt="" class="size-full object-contain" />
    <Icon v-else name="lucide:puzzle" class="size-1/2 text-muted-foreground" />
  </span>
</template>
