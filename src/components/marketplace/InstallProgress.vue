<script setup lang="ts">
/**
 * The download of the bundle that is being installed (spec 069, FR-015): a progress bar and
 * "Abbrechen", and the refusal of a second request or the failure of the download as a line that
 * can be dismissed. The install dialog itself is shown by the frame once the host holds the
 * checked bundle.
 */
const { t } = useI18n()
const { target, progress, downloading, failure, cancelAsync, dismissFailure } =
  useMarketplaceInstall()

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

    <p
      v-if="failure"
      class="flex items-start gap-2 rounded-xl bg-destructive/10 px-4 py-3 text-sm text-destructive"
      role="alert"
      data-testid="marketplace-install-error"
    >
      <span class="min-w-0 flex-1">{{ failure }}</span>
      <UiButton
        variant="ghost"
        size="icon"
        :aria-label="t('fields.clear')"
        data-testid="marketplace-install-error-dismiss"
        @click="dismissFailure"
      >
        <Icon name="lucide:x" class="size-4" />
      </UiButton>
    </p>
  </div>
</template>
