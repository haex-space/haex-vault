import { computed, reactive, ref, type Ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import type { ToolAvailabilityEvent } from '~/composables/useChat'
import { topicKeyOf } from '~/lib/agents/rights'
import {
  createToolNotices,
  delegateNoticeKey,
  noticeKey,
  restrictedKey,
  type ProfileRestrictedEvent,
} from '~/lib/chat/toolNotice'
import type { AgentKind } from '~/types/bindings/AgentKind'
import type { DelegateStatus } from '~/types/bindings/DelegateStatus'

// One memory for the whole app session, shared by every chat tab (see `createToolNotices`).
const notices = createToolNotices()

const DELEGATE_NAMES: Partial<Record<AgentKind, string>> = {
  claudeCodeDelegate: 'Claude Code',
  codexDelegate: 'Codex',
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

/**
 * The tool-use notice of the open conversation: set by `chat-tool-availability` the first time
 * a state shows up for a conversation, hidden when another conversation is open, closed by
 * `dismiss`. A call the agent profile of the model turned away shows a notice of its own, once per
 * conversation and profile (spec 083 US5). A delegate has holzi's tools as far as the agent profile of its
 * model allows (spec 083 FR-029), so its turn needs a notice only while the MCP access is off.
 */
export function useToolNotice(activeThreadId: Ref<string | null>) {
  const current = ref<{ threadId: string; notice: ToolNotice } | null>(null)

  function show(threadId: string, key: string, delegate?: AgentKind) {
    current.value = {
      threadId,
      notice: {
        key,
        params: { name: delegate ? (DELEGATE_NAMES[delegate] ?? '') : '' },
      },
    }
  }

  async function showDelegateAsync(threadId: string, kind: AgentKind) {
    const status = await invoke<DelegateStatus>('mcp_delegate_status', {
      args: { kind },
    }).catch((error: unknown) => {
      console.warn('[chat] delegate status failed', error)
      return null
    })
    // Without a status the notice of spec 032 stays: the delegate runs without holzi's tools.
    const key = status ? delegateNoticeKey(status) : noticeKey('delegate')
    if (!key || !notices.show(threadId, 'delegate')) return
    show(threadId, key, status ? kind : undefined)
  }

  function handleToolAvailability(e: ToolAvailabilityEvent) {
    if (e.state === 'delegate' && e.delegateKind) {
      void showDelegateAsync(e.threadId, e.delegateKind)
      return
    }
    const key = noticeKey(e.state)
    if (key && notices.show(e.threadId, e.state)) show(e.threadId, key)
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

  /** The notice to show now, if any. */
  const toolNotice = computed(
    () =>
      (activeThreadId.value && restrictedByThread.get(activeThreadId.value)) ||
      (current.value && current.value.threadId === activeThreadId.value
        ? current.value.notice
        : null),
  )

  function dismissToolNotice() {
    if (activeThreadId.value) restrictedByThread.delete(activeThreadId.value)
    current.value = null
  }

  return {
    toolNotice,
    handleToolAvailability,
    handleProfileRestricted,
    dismissToolNotice,
  }
}
