<script setup lang="ts">
/**
 * Transcript rendering (messages, reasoning accordions, tool call/result
 * rows, delegate/autonomy/audit-marker labels) — extracted from
 * `src/pages/chat/[instance].vue` (spec 015-workspace-shell, T011, plan
 * research R10). Presentational: derives its labels from props only, emits
 * `toggleReasoning` rather than owning `expandedReasoning` itself, since
 * that state is also cleared from outside the transcript (new chat, thread
 * delete).
 */
import DOMPurify from 'dompurify'
import { marked } from 'marked'
import type { Message } from '~/composables/useChat'
import { fromToolName } from '~/lib/actions/agentTools'
import { ALL_ACTIONS } from '~/lib/actions/catalog'
import { originLabel } from '~/lib/chat/toolOrigin'
import { prettyToolText } from '~/lib/chat/toolText'
import type { Provider } from '~/composables/useProviders'

const props = defineProps<{
  activeMessages: Message[]
  showModelSelection: boolean
  activeModelName: string | undefined
  loadingModelName: string | null
  streamingMessageId: string | null
  retryingMessageId: string | null
  reasoningByMessage: Record<string, string>
  expandedReasoning: Set<string>
  providerList: Provider[]
}>()

const emit = defineEmits<{
  toggleReasoning: [messageId: string, expanded: boolean]
}>()

const { t, locale } = useI18n()

/** Tool rows of an action show the action's localized title, an extension's tool its extension
 * and title (spec 018), other tools their own name. */
function toolLabel(m: Message): string {
  if (m.toolOrigin) return originLabel(m.toolOrigin, locale.value)
  if (m.toolSource !== 'action') return m.toolName ?? ''
  const action = fromToolName(m.toolName ?? '', ALL_ACTIONS)
  return action ? t(action.titleKey) : (m.toolName ?? '')
}

/** Markers `approval_bridge.rs` persists as a `gated-permissive` audit
 * row's content (spec 009-autonomous-delegate-mode US2) — fixed and
 * non-localized on the wire, translated here at the same i18n boundary
 * other fixed markers use (CONTEXT.md). */
const DENY_AUDIT_MARKERS = new Set([
  'gated_permissive_call_permitted',
  'denied_by_deny_rule',
  'declined_by_user',
])

function reasoningFor(messageId: string): string {
  return props.reasoningByMessage[messageId] ?? ''
}

/**
 * Localized "Answered by Claude Code"/"Answered by Codex" label for a
 * delegate-answered message, or `null` for any other backend (spec.md
 * FR-005) — derived from the message's existing `modelId`
 * (`<providerId>:<remoteId>`) via the provider's `adapter` (the stable
 * `claude`/`codex` vendor discriminator, `DelegateVendor::as_str()`), not
 * `remoteId` — Claude's `remoteId` is a real Anthropic model id (e.g.
 * `claude-opus-5`) rather than a fixed vendor tag, so it can't be
 * compared against `'claude'`/`'codex'` directly.
 */
function delegateAnsweredByLabel(modelId: string | null): string | null {
  if (!modelId) return null
  const [providerId] = modelId.split(':')
  const provider = props.providerList.find(
    (p) => p.id === providerId && p.kind === 'cli_delegate',
  )
  if (!provider) return null
  const vendor = provider.adapter
  if (vendor !== 'claude' && vendor !== 'codex') return null
  return t('chat.model.answeredByDelegate', {
    name: t(`chat.model.delegate.${vendor}`),
  })
}

/** Translates gated-permissive audit markers for transcript display. */
function toolResultContentLabel(m: Message): string {
  if (m.role === 'tool_result' && DENY_AUDIT_MARKERS.has(m.content)) {
    return t(`chat.autonomy.audit.${m.content}`)
  }
  return m.content || (props.streamingMessageId === m.id ? '…' : '')
}

function renderMarkdown(content: string): string {
  return DOMPurify.sanitize(
    marked.parse(content, { async: false, breaks: true }),
  )
}
</script>

<template>
  <ChatModelSelection v-if="showModelSelection" />
  <template v-else>
    <div
      v-if="activeMessages.length === 0"
      class="mx-auto flex h-full max-w-3xl flex-col items-center justify-center text-center"
    >
      <div
        class="mb-4 flex h-12 w-12 items-center justify-center rounded-2xl bg-foreground text-background"
      >
        <Icon name="lucide:sparkles" class="h-5 w-5" />
      </div>
      <h2 class="text-xl font-semibold tracking-tight">
        {{ t('chat.empty.title') }}
      </h2>
      <p class="mt-2 max-w-md text-sm text-muted-foreground">
        {{
          t('chat.empty.description', {
            modelName: activeModelName ?? loadingModelName,
          })
        }}
      </p>
    </div>
    <div
      v-for="m in activeMessages"
      :key="m.id"
      class="mx-auto mb-6 flex max-w-3xl gap-3"
      :class="m.role === 'user' ? 'justify-end' : 'justify-start'"
    >
      <div
        v-if="m.role !== 'user'"
        class="mt-1 hidden h-7 w-7 shrink-0 items-center justify-center rounded-lg bg-foreground text-background sm:flex"
      >
        <Icon
          :name="m.role === 'assistant' ? 'lucide:sparkles' : 'lucide:wrench'"
          class="h-3.5 w-3.5"
        />
      </div>
      <div
        class="min-w-0 max-w-[min(90%,48rem)]"
        :class="m.role === 'user' ? 'order-first' : ''"
      >
        <div class="mb-1 flex items-center gap-2 text-xs text-muted-foreground">
          <template v-if="m.role === 'tool_call'">
            {{ t('chat.tool.call', { name: toolLabel(m) }) }}
            <WmDevBadge v-if="m.toolOrigin?.dev" />
          </template>
          <template v-else-if="m.role === 'tool_result'">
            {{
              m.toolIsError ? t('chat.tool.resultError') : t('chat.tool.result')
            }}
          </template>
          <template v-else>
            {{
              m.role === 'user'
                ? t('chat.sender.user')
                : m.role === 'assistant'
                  ? t('chat.sender.assistant')
                  : t('chat.sender.system')
            }}
            <span
              v-if="m.role === 'assistant' && m.completionTokens"
              class="ml-2"
            >
              {{ t('chat.tokens', { count: m.completionTokens }) }}
            </span>
            <span
              v-if="
                m.role === 'assistant' && delegateAnsweredByLabel(m.modelId)
              "
              class="ml-2"
            >
              {{ delegateAnsweredByLabel(m.modelId) }}
            </span>
            <span v-if="m.role === 'assistant' && m.autonomyMode" class="ml-2">
              {{ t(`chat.autonomy.${m.autonomyMode}`) }}
            </span>
            <span
              v-if="m.finishReason === 'error'"
              class="ml-2 text-destructive"
            >
              {{ t('chat.errorLabel') }}
            </span>
            <span
              v-if="m.finishReason === 'cancelled'"
              class="ml-2 text-muted-foreground"
            >
              {{ t('chat.cancelledLabel') }}
            </span>
            <span
              v-if="m.finishReason === 'tool_limit_reached'"
              class="ml-2 text-warning"
            >
              {{ t('chat.tool.limitReached') }}
            </span>
            <span
              v-if="retryingMessageId === m.id"
              class="ml-2 text-muted-foreground italic"
            >
              {{ t('chat.retrying') }}
            </span>
          </template>
        </div>
        <div
          class="rounded-2xl px-4 py-3 text-sm leading-6 shadow-sm [overflow-wrap:anywhere]"
          :class="{
            'whitespace-pre-wrap':
              m.role === 'user' ||
              m.role === 'tool_call' ||
              m.role === 'tool_result',
            'bg-foreground text-background': m.role === 'user',
            'border border-border bg-background':
              m.role === 'assistant' || m.role === 'system',
            'rounded-lg bg-muted/30 font-mono text-xs leading-5':
              m.role === 'tool_call' ||
              (m.role === 'tool_result' && !m.toolIsError),
            'rounded-lg bg-destructive/10 text-destructive font-mono text-xs leading-5':
              m.role === 'tool_result' && m.toolIsError,
            'max-h-96 overflow-y-auto':
              m.role === 'tool_call' || m.role === 'tool_result',
          }"
        >
          <template v-if="m.role === 'tool_call'">{{
            prettyToolText(m.toolInput ?? '')
          }}</template>
          <!-- eslint-disable vue/no-v-html -->
          <div
            v-else-if="m.role === 'assistant' || m.role === 'system'"
            class="chat-markdown"
            v-html="
              renderMarkdown(
                m.content || (streamingMessageId === m.id ? '…' : ''),
              )
            "
          />
          <!-- eslint-enable vue/no-v-html -->
          <template v-else>{{
            prettyToolText(toolResultContentLabel(m))
          }}</template>
        </div>
        <ChatReasoningAccordion
          v-if="m.role === 'assistant' && reasoningFor(m.id)"
          :reasoning="reasoningFor(m.id)"
          :label="t('chat.reasoning.title')"
          :expanded="expandedReasoning.has(m.id)"
          @update:expanded="emit('toggleReasoning', m.id, $event)"
        />
      </div>
    </div>
  </template>
</template>
