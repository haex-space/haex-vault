<script setup lang="ts">
/**
 * The way out of the camera scan of a phone. The barcode plugin shows the camera in a native view
 * behind the page and has no button of its own; while a scan runs the page lies transparent above
 * it (plugin option `windowed`), everything else is hidden, and this close button stays.
 *
 * A sheet that is open while the scan runs (the link sheet) treats a tap outside of itself as a
 * wish to close, and keeps the focus inside; the button therefore stops those events before they
 * reach the sheet, and takes the pointer back from the sheet's `pointer-events: none` on the page.
 */
import { toast } from 'vue-sonner'

const { t } = useI18n()
const { active, cancelAsync } = useScanSession()
const { errString } = useErrorString()

// The page marks itself, the style below hides the rest and clears the background.
watch(
  active,
  (running) => {
    document.documentElement.toggleAttribute('data-scanning', running)
  },
  { immediate: true },
)
onBeforeUnmount(() => document.documentElement.removeAttribute('data-scanning'))

async function close() {
  try {
    await cancelAsync()
  } catch (cause) {
    toast.error(errString(cause))
  }
}
</script>

<template>
  <div
    v-if="active"
    data-scan-overlay
    class="pointer-events-none fixed inset-x-0 bottom-0 z-[2147483647] flex justify-center p-4"
    style="padding-bottom: calc(env(safe-area-inset-bottom, 0px) + 1.5rem)"
  >
    <UiButton
      type="button"
      variant="secondary"
      size="lg"
      class="pointer-events-auto rounded-full px-6"
      data-testid="scan-close"
      @pointerdown.stop
      @focusin.stop
      @click.stop="close"
    >
      <Icon name="lucide:x" class="size-5" />
      {{ t('onboarding.link.camera.close') }}
    </UiButton>
  </div>
</template>

<style>
/* The camera is behind the page: nothing of the app may cover it while a scan runs. */
html[data-scanning],
html[data-scanning] body {
  background: transparent !important;
}
html[data-scanning] body > * {
  visibility: hidden !important;
}
html[data-scanning] [data-scan-overlay],
html[data-scanning] [data-scan-overlay] * {
  visibility: visible !important;
}
</style>
