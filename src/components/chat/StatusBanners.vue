<script setup lang="ts">
/**
 * The three dismissible/status banners below the header (composer/model
 * error with retry actions, autonomy-mode preference error, model
 * loading status) — extracted from `src/pages/chat/[instance].vue` (spec
 * 015-workspace-shell, T013, to keep the orchestrator page under the
 * 500-line constitution limit). Presentational only. The tool-use notice of a delegate offers
 * "Rechte festlegen" (spec 021 US6).
 */
import type { ToolNotice } from '~/composables/useToolNotice'

defineProps<{
  displayedError: string | null
  canRetrySend: boolean
  loadErrorModelId: string | null
  autonomyPreferenceError: string | null
  autonomyPreferenceLoading: boolean
  loadingLabel: string | null
  /** The tool-use notice to show (spec 032 US4, spec 021 US6), or `null`. */
  toolNotice?: ToolNotice | null
}>()

const emit = defineEmits<{
  retrySend: []
  retryModelLoad: []
  dismissError: []
  retryAutonomyMode: []
  dismissToolNotice: []
  setDelegateRights: []
}>()

const { t } = useI18n()
</script>

<template>
  <div
    v-if="displayedError"
    class="border-b border-destructive/20 bg-destructive/10 p-3 text-sm text-destructive flex items-start justify-between gap-2"
  >
    <span>{{ displayedError }}</span>
    <span class="flex gap-2 shrink-0">
      <button
        v-if="canRetrySend"
        class="text-xs underline"
        @click="emit('retrySend')"
      >
        {{ t('chat.retry') }}
      </button>
      <button
        v-if="loadErrorModelId"
        class="text-xs underline"
        @click="emit('retryModelLoad')"
      >
        {{ t('chat.loading.retry') }}
      </button>
      <button class="text-xs underline" @click="emit('dismissError')">
        {{ t('chat.close') }}
      </button>
    </span>
  </div>

  <div
    v-if="autonomyPreferenceError"
    class="border-b border-destructive/20 bg-destructive/10 p-3 text-sm text-destructive flex items-start justify-between gap-2"
    role="alert"
  >
    <span
      >{{ t('settings.autonomyMode.loadFailed') }}:
      {{ autonomyPreferenceError }}</span
    >
    <button
      class="text-xs underline shrink-0"
      :disabled="autonomyPreferenceLoading"
      @click="emit('retryAutonomyMode')"
    >
      {{ t('chat.loading.retry') }}
    </button>
  </div>

  <div
    v-if="toolNotice"
    class="border-b border-primary/20 bg-primary/10 p-3 text-sm text-primary flex items-start justify-between gap-2"
    role="status"
    data-testid="chat-tool-notice"
  >
    <span class="space-y-1">
      <span class="block">{{ t(toolNotice.key, toolNotice.params) }}</span>
      <span
        v-if="toolNotice.failure"
        class="block text-destructive"
        role="alert"
        >{{ toolNotice.failure }}</span
      >
    </span>
    <span class="flex gap-2 shrink-0">
      <button
        v-if="toolNotice.setRights"
        type="button"
        class="text-xs underline font-medium"
        data-testid="chat-delegate-set-rights"
        @click="emit('setDelegateRights')"
      >
        {{ t('chat.toolNotice.setRights') }}
      </button>
      <button
        type="button"
        class="text-xs underline"
        @click="emit('dismissToolNotice')"
      >
        {{ t('chat.close') }}
      </button>
    </span>
  </div>

  <div
    v-if="loadingLabel"
    class="border-b border-primary/20 bg-primary/10 p-3 text-sm text-primary"
    role="status"
  >
    {{ loadingLabel }}
  </div>
</template>
