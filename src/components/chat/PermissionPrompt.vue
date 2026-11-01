<script setup lang="ts">
import type { PendingPrompt } from '~/lib/chat/prompts'
import { describeToolCall, type ResolveTarget } from '~/lib/actions/agentTools'
import { ALL_ACTIONS } from '~/lib/actions/catalog'
import { inputRows, originLabel } from '~/lib/chat/toolOrigin'
import { useWindowManagerStore } from '~/stores/windowManager'

const { t, locale } = useI18n()

const props = defineProps<{
  mode: 'manual' | 'auto' | 'plan'
  disabled?: boolean
  /** Oldest-first queue — only the first is shown; more than one can be
   * pending at once (spec.md Edge Case: independent tool calls each get
   * their own request). */
  pendingPrompts: PendingPrompt[]
  /** The open thread: its delegate's calls to holzi are asked here too (spec 021 US6). */
  threadId?: string | null
}>()

/** The approval shown now: the oldest prompt, when it asks for one. */
const approval = computed(() => {
  const first = props.pendingPrompts[0]
  return first?.kind === 'approval' ? first : undefined
})

const wm = useWindowManagerStore()

type TabInfo = ReturnType<typeof wm.tabDisplayInfo>

function titleFrom(
  info: Pick<TabInfo, 'titleOverride' | 'titleKey' | 'titleParams'>,
): string {
  return (
    info.titleOverride ??
    (info.titleKey ? t(info.titleKey, info.titleParams) : '')
  )
}

/** Names what an action acts on as the tab bar and the overviews name it. */
const resolveTarget: ResolveTarget = (kind, id) => {
  if (kind === 'workspace') {
    const position = wm.workspaces.findIndex((w) => w.id === id)
    return position < 0
      ? undefined
      : t('wm.workspaces.numbered', { number: position + 1 })
  }
  if (kind === 'window') {
    const window = wm.windows.find((w) => w.id === id)
    const info = window ? wm.windowDisplayInfo(window) : null
    return info ? titleFrom(info) : undefined
  }
  const tab = wm.windows.flatMap((w) => w.tabs).find((item) => item.id === id)
  return tab ? titleFrom(wm.tabDisplayInfo(tab)) : undefined
}

const description = computed(() => {
  const shown = approval.value
  if (!shown || shown.toolSource !== 'action') return undefined
  return describeToolCall(
    shown.toolName,
    shown.toolInput,
    ALL_ACTIONS,
    resolveTarget,
  )
})

/** An extension's tool (spec 018): "<extension>: <tool title>" and its input as fields. */
const extensionTool = computed(() => {
  const shown = approval.value
  if (!shown?.toolOrigin) return undefined
  return {
    title: originLabel(shown.toolOrigin, locale.value),
    inputs: inputRows(shown.toolInput),
  }
})

const permissionModeLabel = computed(() => t(`chat.permission.${props.mode}`))

const emit = defineEmits<{
  'update:mode': [mode: 'manual' | 'auto' | 'plan']
  allow: [requestId: string]
  deny: [requestId: string]
  cancel: []
}>()

/**
 * The dialog is controlled purely by `pendingPrompts` (owned by the
 * parent), so `open` is always `true` while it is mounted. Reka UI's Dialog
 * respects that as a controlled prop — the built-in close button, Escape,
 * and outside-click can request a close via `update:open`, but nothing
 * visually closes until the approval itself disappears. Treat that
 * request the same as the explicit "stop generating" button: it must never
 * silently drop a still-pending approval (matches the existing
 * cancel-not-deny semantics tested on the backend).
 */
function onUpdateOpen(open: boolean) {
  if (!open) emit('cancel')
}
</script>

<template>
  <ChatComposerControl
    :label="t('chat.permission.modeLabel')"
    :value="mode"
    :display-value="permissionModeLabel"
    icon="lucide:shield-check"
    control-id="permission-mode"
    :options="[
      { value: 'plan', label: t('chat.permission.plan') },
      { value: 'manual', label: t('chat.permission.manual') },
      { value: 'auto', label: t('chat.permission.auto') },
    ]"
    :disabled="disabled"
    @update:value="emit('update:mode', $event as 'manual' | 'auto' | 'plan')"
  />

  <ChatDelegateCallPrompt :thread-id="threadId ?? null" :held="!!approval" />

  <UiDrawerModal
    v-if="approval"
    :open="true"
    :title="
      t('chat.permission.requestTitle', {
        name: description
          ? t(description.titleKey)
          : (extensionTool?.title ?? approval.toolName),
      })
    "
    @update:open="onUpdateOpen"
  >
    <template #content>
      <div class="space-y-3">
        <div
          class="text-xs"
          :class="
            approval.riskClass === 'risky'
              ? 'text-destructive'
              : 'text-muted-foreground'
          "
        >
          {{ t(`chat.permission.${approval.riskClass}`) }}
        </div>
        <dl v-if="description" class="space-y-1 text-sm">
          <div v-if="description.target" class="flex gap-2">
            <dt class="text-muted-foreground">
              {{ t(`chat.permission.target.${description.target.kind}`) }}:
            </dt>
            <dd>{{ description.target.label }}</dd>
          </div>
          <div
            v-for="[field, value] in description.inputs"
            :key="field"
            class="flex gap-2"
          >
            <dt class="text-muted-foreground">{{ field }}:</dt>
            <dd class="break-words">{{ value }}</dd>
          </div>
        </dl>
        <dl v-else-if="extensionTool" class="space-y-1 text-sm">
          <div
            v-for="[field, value] in extensionTool.inputs"
            :key="field"
            class="flex gap-2"
          >
            <dt class="text-muted-foreground">{{ field }}:</dt>
            <dd class="break-words">{{ value }}</dd>
          </div>
        </dl>
        <pre
          v-else
          class="text-xs bg-muted/30 rounded p-2 overflow-x-auto whitespace-pre-wrap"
          >{{ JSON.stringify(approval.toolInput, null, 2) }}</pre>
      </div>
    </template>
    <template #footer>
      <div class="flex justify-end gap-2">
        <UiButton size="sm" variant="outline" @click="emit('cancel')">
          {{ t('chat.permission.stopGenerating') }}
        </UiButton>
        <UiButton
          size="sm"
          variant="outline"
          @click="emit('deny', approval.requestId)"
        >
          {{ t('chat.permission.deny') }}
        </UiButton>
        <UiButton size="sm" @click="emit('allow', approval.requestId)">
          {{ t('chat.permission.allow') }}
        </UiButton>
      </div>
    </template>
  </UiDrawerModal>
</template>
