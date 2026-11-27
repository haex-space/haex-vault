import { computed, reactive, ref, type Ref } from 'vue'
import type { ToolAvailabilityEvent } from '~/composables/useChat'
import { topicKeyOf } from '~/lib/agents/rights'
import {
  createToolNotices,
  type CompactionEvent,
  DELEGATE_OWN_WEB_KEY,
  DELEGATE_UNAVAILABLE_KEY,
  type DelegateOwnWebEvent,
  type DelegateToolsUnavailableEvent,
  noticeKey,
  restrictedKey,
  type ProfileRestrictedEvent,
} from '~/lib/chat/toolNotice'
import type { AgentKind } from '~/types/bindings/AgentKind'

// One memory for the whole app session, shared by every chat tab (see `createToolNotices`).
const notices = createToolNotices()

const DELEGATE_NAMES: Partial<Record<AgentKind, string>> = {
  claudeCodeDelegate: 'Claude Code',
  codexDelegate: 'Codex',
  abacusDelegate: 'Abacus AI',
}

/** The notice the chat shows now. */
export interface ToolNotice {
  key: string
  params: Record<string, string>
  /** The agent profile that turned a call away (spec 083 US5): the notice names it and offers to
   * open it. */
  profile?: { id: string; name: string }
  /** The i18n key of the topic's name, for the notice of a topic the profile does not give. */
  topicKey?: string
  /** The name of the extension the profile does not give. */
  extension?: string
}

// Profile notices are shared by all mounted chat tabs. The event listener that receives an event
// first must not keep the only copy in its own tab's local state.
const restrictedByThread = reactive(new Map<string, ToolNotice>())
// A delegate's residual own-web notice must survive notices emitted by other threads and profile
// restrictions in the same thread until that conversation can show and dismiss it.
const ownWebByThread = reactive(new Map<string, ToolNotice>())

/**
 * The tool-use notice of the open conversation: set by `chat-tool-availability` the first time
 * a state shows up for a conversation, hidden when another conversation is open, closed by
 * `dismiss`. A call the agent profile of the model turned away shows a notice of its own, once per
 * conversation and profile (spec 083 US5). A delegate has haex-vault's tools on every start as far as the
 * agent profile of its model allows (spec 102 FR-004, spec 083 FR-029); its turn needs a notice only when
 * a start had none, or when it used its own web access (spec 102 FR-012, FR-013).
 */
export function useToolNotice(activeThreadId: Ref<string | null>) {
  const current = ref<{ threadId: string; notice: ToolNotice } | null>(null)

  function show(
    threadId: string,
    key: string,
    delegate?: AgentKind,
    extra: Record<string, string> = {},
  ) {
    current.value = {
      threadId,
      notice: {
        key,
        params: {
          name: delegate ? (DELEGATE_NAMES[delegate] ?? '') : '',
          ...extra,
        },
      },
    }
  }

  function handleToolAvailability(e: ToolAvailabilityEvent) {
    const key = noticeKey(e.state)
    if (key && notices.show(e.threadId, e.state)) show(e.threadId, key)
  }

  /** A delegate's start has no haex-vault tools (spec 102 FR-012): the notice with the reason, once
   * per conversation and reason. */
  function handleDelegateToolsUnavailable(e: DelegateToolsUnavailableEvent) {
    if (notices.showUnavailable(e.threadId, e.reason))
      show(e.threadId, DELEGATE_UNAVAILABLE_KEY, e.delegateKind, {
        reason: e.reason,
      })
  }

  /** A delegate used its own web access, which its program cannot block (spec 102 FR-013): the
   * notice, once per conversation. */
  function handleDelegateOwnWeb(e: DelegateOwnWebEvent) {
    if (!notices.showOwnWeb(e.threadId)) return
    ownWebByThread.set(e.threadId, {
      key: DELEGATE_OWN_WEB_KEY,
      params: { name: DELEGATE_NAMES[e.delegateKind] ?? '' },
    })
  }

  /** A call the agent profile of the chat's model turned away: the notice, once per conversation
   * and profile (spec 083 US5). */
  function handleProfileRestricted(e: ProfileRestrictedEvent) {
    if (!notices.showRestricted(e.threadId, e.profileId, e.missing)) return
    restrictedByThread.set(e.threadId, {
      key: restrictedKey(e.missing),
      params: {},
      profile: { id: e.profileId, name: e.profileName },
      topicKey:
        e.missing.kind === 'topic' || e.missing.kind === 'topicWrite'
          ? (topicKeyOf(e.missing.topic) ?? undefined)
          : undefined,
      extension: e.missing.kind === 'extension' ? e.missing.name : undefined,
    })
  }

  /** The built-in agent summarises older messages (spec 086): a line while it writes the summary, and
   * one notice when it could not, which says the request was shortened another way. */
  function handleCompaction(e: CompactionEvent) {
    if (e.state === 'summarising') {
      show(e.threadId, 'chat.compaction.summarising')
    } else if (e.state === 'failed') {
      show(e.threadId, 'chat.compaction.failed')
    } else if (
      current.value?.threadId === e.threadId &&
      current.value.notice.key === 'chat.compaction.summarising'
    ) {
      current.value = null
    }
  }

  /** The notice to show now, if any. */
  const toolNotice = computed(
    () =>
      (activeThreadId.value && ownWebByThread.get(activeThreadId.value)) ||
      (activeThreadId.value && restrictedByThread.get(activeThreadId.value)) ||
      (current.value && current.value.threadId === activeThreadId.value
        ? current.value.notice
        : null),
  )

  function dismissToolNotice() {
    const threadId = activeThreadId.value
    if (!threadId) return
    if (ownWebByThread.delete(threadId)) return
    if (restrictedByThread.delete(threadId)) return
    if (current.value?.threadId === threadId) current.value = null
  }

  return {
    toolNotice,
    handleToolAvailability,
    handleDelegateToolsUnavailable,
    handleDelegateOwnWeb,
    handleProfileRestricted,
    handleCompaction,
    dismissToolNotice,
  }
}
