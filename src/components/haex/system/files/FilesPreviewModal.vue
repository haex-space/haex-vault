<template>
  <!--
    Inline media preview for the active `browser.preview`. The URL may be
    a blob: (small local files), an asset: (regular local files), or a
    haex-stream: (S3 audio/video). The audio/video elements drive their
    own Range requests when the source supports them — we just hand over
    the URL.
  -->
  <UModal
    :open="browser.preview.isOpen.value"
    :title="browser.preview.previewFilename.value ?? ' '"
    :fullscreen="isPreviewMaximized"
    :ui="{ content: isPreviewMaximized ? '' : 'max-w-3xl' }"
    @update:open="onPreviewOpenChange"
  >
    <template #actions>
      <UButton
        v-if="browser.preview.previewType.value !== 'audio'"
        :icon="isPreviewMaximized ? 'i-lucide-minimize' : 'i-lucide-maximize'"
        color="neutral"
        variant="ghost"
        :aria-label="isPreviewMaximized ? t('restorePreview') : t('maximizePreview')"
        @click="togglePreviewMaximized"
      />
    </template>
    <template #body>
      <div
        :class="[
          'flex items-center justify-center',
          isPreviewMaximized ? 'h-full' : 'min-h-32',
        ]"
      >
        <audio
          v-if="
            browser.preview.previewType.value === 'audio' &&
              browser.preview.previewUrl.value
          "
          data-testid="file-preview-audio"
          controls
          autoplay
          class="w-full"
          :src="browser.preview.previewUrl.value"
          @error="onMediaError"
        />
        <video
          v-else-if="
            browser.preview.previewType.value === 'video' &&
              browser.preview.previewUrl.value
          "
          data-testid="file-preview-video"
          controls
          autoplay
          :class="isPreviewMaximized ? 'max-h-full w-full' : 'max-h-[70vh] w-full'"
          :src="browser.preview.previewUrl.value"
          @error="onMediaError"
        />
        <img
          v-else-if="
            browser.preview.previewType.value === 'image' &&
              browser.preview.previewUrl.value
          "
          :src="browser.preview.previewUrl.value"
          :alt="browser.preview.previewFilename.value ?? ''"
          :class="isPreviewMaximized ? 'max-h-full object-contain' : 'max-h-[70vh] object-contain'"
        >
        <iframe
          v-else-if="
            browser.preview.previewType.value === 'pdf' &&
              browser.preview.previewUrl.value
          "
          :src="browser.preview.previewUrl.value"
          :class="isPreviewMaximized ? 'w-full h-full border-0' : 'w-full h-[70vh] border-0'"
        />
      </div>
    </template>
  </UModal>
</template>

<script setup lang="ts">
import { getCurrentWindow } from '@tauri-apps/api/window'
import type { useFileBrowser } from '~/composables/useFileBrowser'

const props = defineProps<{
  browser: ReturnType<typeof useFileBrowser>
}>()

const { t } = useI18n()
const toast = useToast()

// --- Media preview error handling ---
//
// WebKitGTK (Linux) decodes <audio>/<video> via GStreamer. Patent-encumbered
// formats like H.264/AAC (.mp4) need gstreamer1.0-libav, which isn't part of
// the base install on many distros — without it the player just shows a black
// frame and stays silent. Surface a clear, actionable message instead.
const onMediaError = (event: Event) => {
  const el = event.target as HTMLMediaElement | null
  const code = el?.error?.code
  const isFormatError =
    code === MediaError.MEDIA_ERR_DECODE ||
    code === MediaError.MEDIA_ERR_SRC_NOT_SUPPORTED
  toast.add({
    title: t('mediaPlaybackFailed'),
    description: isFormatError
      ? t('mediaCodecMissing')
      : el?.error?.message || undefined,
    color: 'error',
  })
}

// Fullscreen preview. We deliberately avoid the browser Fullscreen API
// (`video.requestFullscreen()`) — it aborts WebKitGTK on some Linux/NVIDIA
// setups, so it's disabled in the Rust webview setup. Instead we drive the
// real OS window into fullscreen via Tauri's window API (GTK's
// `gtk_window_fullscreen`, a different code path that doesn't crash) and let
// the modal fill that window via Nuxt UI's `fullscreen` prop.
const isPreviewMaximized = ref(false)

const setPreviewMaximized = async (maximized: boolean) => {
  isPreviewMaximized.value = maximized
  try {
    await getCurrentWindow().setFullscreen(maximized)
  } catch {
    // Window fullscreen is best-effort (e.g. unsupported platform); the modal
    // still expands to fill the window either way.
  }
}

const togglePreviewMaximized = () => setPreviewMaximized(!isPreviewMaximized.value)

const onPreviewOpenChange = (open: boolean) => {
  if (!open) props.browser.preview.close()
}

// Exit OS fullscreen whenever the preview closes — by any path. A programmatic
// `preview.close()` (e.g. breadcrumb navigation via navigateToRoot) flips
// `isOpen` without emitting the modal's `update:open`, so resetting here rather
// than in `onPreviewOpenChange` covers both the user-driven and code-driven
// close.
watch(
  () => props.browser.preview.isOpen.value,
  (open) => {
    if (!open && isPreviewMaximized.value) void setPreviewMaximized(false)
  },
)

// Safety net: if this view unmounts while still maximized (e.g. navigating
// away without closing the modal), the OS window would stay fullscreen.
onBeforeUnmount(() => {
  if (isPreviewMaximized.value) void setPreviewMaximized(false)
})
</script>
