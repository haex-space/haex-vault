<script setup lang="ts">
/**
 * The download of the bundle that is being installed (spec 069, FR-015): a progress bar and
 * "Abbrechen", and the refusal of a second request or the failure of the download as a toast.
 * The install dialog itself is shown by the frame once the host holds the checked bundle.
 */
import { toast } from 'vue-sonner'

const { t } = useI18n()
const { target, progress, downloading, failure, cancelAsync, dismissFailure } =
  useMarketplaceInstall()

// The failure goes out as a toast and is dropped, so the same one again shows again.
watch(
  failure,
  (message) => {
    if (!message) return
    toast.error(message)
    dismissFailure()
  },
  { immediate: true },
)

/** `null` while the total is not known: the bar then runs without an end. */
const percent = computed(() => {
  const current = progress.value
  if (!current || current.total <= 0) return null
  return Math.min(100, Math.floor((current.loaded / current.total) * 100))
})
</script>

<template>
  <div class="flex flex-col gap-2 px-3 pb-3 empty:hidden">
    <section
      v-if="downloading && target"
      class="flex flex-col gap-2 rounded-xl border bg-background p-4"
      data-testid="marketplace-progress"
      role="status"
    >
      <p class="text-sm font-medium">
        {{
          t('marketplace.progress.title', {
            name: target.name,
            version: target.version,
          })
        }}
      </p>
      <UiProgress
        :value="percent"
        :label="
          t('marketplace.progress.title', {
            name: target.name,
            version: target.version,
          })
        "
      />
      <div class="flex items-center justify-between gap-2">
        <span class="text-xs text-muted-foreground">{{
          percent === null ? '' : t('marketplace.progress.percent', { percent })
        }}</span>
        <UiButton
          variant="outline"
          size="sm"
          data-testid="marketplace-progress-cancel"
          @click="cancelAsync"
        >
          {{ t('marketplace.progress.cancel') }}
        </UiButton>
      </div>
    </section>
  </div>
</template>
