<script setup lang="ts">
/**
 * The status bars below the header (the actions of a composer/model error, the retry of a
 * the model loading status; the error texts show as toasts) —
 * extracted from `src/pages/chat/[instance].vue` (spec 015-workspace-shell, T013, to keep the
 * orchestrator page under the 500-line constitution limit). Presentational only. The tool-use notice of a delegate says only that the
 * MCP access is off (spec 083 FR-029).
 */
import { toast } from 'vue-sonner'
import type { ToolNotice } from '~/composables/useToolNotice'
import { profileLabel } from '~/lib/agents/profiles'

const props = defineProps<{
  displayedError: string | null
  canRetrySend: boolean
  loadErrorModelId: string | null
  loadingLabel: string | null
  /** The tool-use notice to show (spec 032 US4, spec 021 US6), or `null`. */
  toolNotice?: ToolNotice | null
}>()

const emit = defineEmits<{
  retrySend: []
  retryModelLoad: []
  dismissError: []
  dismissToolNotice: []
  openProfile: [profileId: string]
}>()

const { t } = useI18n()

/** The text of the notice: a notice of an agent profile names it ("Standard" translated), the topic
 * or the extension it lacks (spec 083 US5). */
const noticeText = computed(() => {
  const notice = props.toolNotice
  if (!notice) return ''
  return t(notice.key, {
    ...notice.params,
    profile: notice.profile
      ? profileLabel(t, notice.profile.id, notice.profile.name)
      : '',
    topic: notice.topicKey ? t(notice.topicKey) : '',
    name: notice.extension ?? notice.params.name ?? '',
  })
})

// The error texts show as toasts, once per new error; the bars below keep only their actions
// while the error lasts.
watch(
  () => props.displayedError,
  (error) => {
    // A fixed id: the model error is shared, so two open chat windows show it once.
    if (error) toast.error(error, { id: 'chat-error' })
  },
  { immediate: true },
)
</script>

<template>
  <div
    v-if="displayedError"
    class="border-b border-destructive/20 bg-destructive/10 p-3 text-sm text-destructive flex items-start justify-end gap-2"
  >
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
    v-if="toolNotice"
    class="border-b border-primary/20 bg-primary/10 p-3 text-sm text-primary flex items-start justify-between gap-2"
    role="status"
    data-testid="chat-tool-notice"
  >
    <span>{{ noticeText }}</span>
    <span class="flex gap-2 shrink-0">
      <button
        v-if="toolNotice.profile"
        type="button"
        class="text-xs underline font-medium"
        data-testid="chat-profile-open"
        @click="emit('openProfile', toolNotice.profile.id)"
      >
        {{ t('chat.agentProfile.restricted.open') }}
      </button>
      <button
        v-if="toolNotice.retry"
        type="button"
        class="text-xs underline font-medium"
        @click="toolNotice.retry()"
      >
        {{ t('chat.toolNotice.retry') }}
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
