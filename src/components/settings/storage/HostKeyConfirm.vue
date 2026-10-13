<script setup lang="ts">
/**
 * The host key of an SFTP server to confirm (spec 050 US4, research R5): the fingerprint the
 * server showed, to compare with what the provider publishes, and for a changed key the one
 * confirmed before. holzi sends no credentials to a server whose key the user has not confirmed
 * (FR-012); confirming takes exactly the key shown here.
 */
import type { ShownHostKey } from '@bindings/ShownHostKey'

defineProps<{
  shown: ShownHostKey
  /** The fingerprint confirmed before, when the server now shows another key. */
  previous?: string
  /** A hint where the provider publishes its fingerprints. */
  hint?: string | null
  busy?: boolean
}>()
const emit = defineEmits<{ confirm: []; cancel: [] }>()
const { t } = useI18n()
</script>

<template>
  <div
    class="flex flex-col gap-3 rounded-xl border px-4 py-3"
    :class="previous ? 'border-destructive' : 'border-border'"
    role="alert"
    data-testid="storage-host-key"
  >
    <p class="text-sm">
      {{
        previous
          ? t('settings.storage.hostKey.changed')
          : t('settings.storage.hostKey.unknown')
      }}
    </p>
    <dl class="grid grid-cols-[auto_1fr] gap-x-3 gap-y-1 text-sm">
      <template v-if="previous">
        <dt class="text-muted-foreground">
          {{ t('settings.storage.hostKey.previous') }}
        </dt>
        <dd
          class="font-mono text-xs break-all"
          data-testid="storage-host-key-previous"
        >
          {{ previous }}
        </dd>
      </template>
      <dt class="text-muted-foreground">
        {{ t('settings.storage.hostKey.shown') }}
      </dt>
      <dd
        class="font-mono text-xs break-all"
        data-testid="storage-host-key-fingerprint"
      >
        {{ shown.fingerprint }}
      </dd>
    </dl>
    <p class="text-sm text-muted-foreground">
      {{ hint ?? t('settings.storage.hostKey.compareHint') }}
    </p>
    <div class="flex flex-wrap gap-2">
      <UiButton
        type="button"
        :variant="previous ? 'destructive' : 'default'"
        :loading="busy"
        data-testid="storage-host-key-confirm"
        @click="emit('confirm')"
      >
        {{
          previous
            ? t('settings.storage.actions.acceptHostKey')
            : t('settings.storage.actions.confirmHostKey')
        }}
      </UiButton>
      <UiButton
        type="button"
        variant="outline"
        :disabled="busy"
        data-testid="storage-host-key-cancel"
        @click="emit('cancel')"
      >
        {{ t('settings.storage.cancel') }}
      </UiButton>
    </div>
  </div>
</template>
