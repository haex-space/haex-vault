<script setup lang="ts">
import { toast } from 'vue-sonner'

/**
 * What a place of the password manager or Wallet shows until the overview has loaded once: a
 * spinner while it loads, and, when the first load failed, a message with a retry (the reason went
 * is toasted here too, under the id the lists use, so it shows once). Without it a failed first load left the spinner running for good.
 */
defineProps<{
  /** The test id of the spinner. */
  testid?: string
}>()

const { t } = useI18n()
const store = usePasswordsStore()
const failed = computed(
  () => !store.hasLoadedOnce && !store.isLoading && store.lastError !== null,
)
watch(
  () => store.lastError,
  (message) => {
    if (message) toast.error(message, { id: 'passwords-load-failed' })
  },
  { immediate: true },
)
</script>

<template>
  <div
    v-if="failed"
    class="flex flex-col items-center gap-3 py-10 text-center text-muted-foreground"
    role="alert"
    data-testid="passwords-load-failed"
  >
    <p>{{ t('passwords.loadFailed') }}</p>
    <UiButton
      variant="outline"
      data-testid="passwords-load-retry"
      @click="store.reloadAsync()"
    >
      {{ t('passwords.loadRetry') }}
    </UiButton>
  </div>
  <div
    v-else
    class="flex justify-center py-10 text-muted-foreground"
    role="status"
    :data-testid="testid"
  >
    <Icon name="lucide:loader-circle" class="size-6 animate-spin" />
    <span class="sr-only">{{ t('passwords.loading') }}</span>
  </div>
</template>
