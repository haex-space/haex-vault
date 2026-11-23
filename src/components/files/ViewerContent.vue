<script setup lang="ts">
/**
 * What the viewer shows of a file (spec 044 FR-009, FR-011, FR-015): text, an image, video, audio
 * or a PDF. The file browser's viewer and the chat's file view (spec 072 US3) both put it in their
 * own frame, so a file looks the same in either. Text comes read, the rest from the media server's
 * `url`; a file the web view cannot play or render reports `failed`, and the frame falls back to
 * its info view.
 */
import type { TextContent } from '@bindings/TextContent'

const props = defineProps<{
  kind: 'text' | 'image' | 'pdf' | 'video' | 'audio'
  text: TextContent | null
  url: string | null
  name: string
}>()

const emit = defineEmits<{ failed: [] }>()

const zoomed = ref(false)
// Another file starts unzoomed.
watch(
  () => props.url,
  () => (zoomed.value = false),
)
</script>

<template>
  <div v-if="kind === 'text' && text" data-testid="files-viewer-text">
    <FilesTextContent :text="text" />
  </div>
  <div
    v-else-if="kind === 'image' && url"
    class="flex min-h-full items-center justify-center p-2"
    data-testid="files-viewer-image"
  >
    <img
      :src="url"
      :alt="name"
      class="cursor-zoom-in"
      :class="
        zoomed
          ? 'max-w-none cursor-zoom-out'
          : 'max-h-full max-w-full object-contain'
      "
      @click="zoomed = !zoomed"
    />
  </div>
  <FilesMediaViewer
    v-else-if="(kind === 'video' || kind === 'audio') && url"
    :url="url"
    :kind="kind"
    @failed="emit('failed')"
  />
  <LazyFilesPdfViewer
    v-else-if="kind === 'pdf' && url"
    :url="url"
    @failed="emit('failed')"
  />
</template>
