import { computed, ref, type Ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import type { ToolAvailabilityEvent } from '~/composables/useChat'
import { useActionOrThrow } from '~/composables/useActionOrThrow'
import { useErrorString } from '~/composables/useErrorString'
import {
  canSetRights,
  createToolNotices,
  delegateNoticeKey,
  noticeKey,
} from '~/lib/chat/toolNotice'
import type { AgentKind } from '~/types/bindings/AgentKind'
import type { DelegateRegistration } from '~/types/bindings/DelegateRegistration'
import type { DelegateStatus } from '~/types/bindings/DelegateStatus'

// One memory for the whole app session, shared by every chat tab (see `createToolNotices`).
const notices = createToolNotices()

const DELEGATE_NAMES: Partial<Record<AgentKind, string>> = {
  claudeCodeDelegate: 'Claude Code',
  codexDelegate: 'Codex',
}

/** The notice the chat shows now: its text, and whether it offers "Rechte festlegen". */
export interface ToolNotice {
  key: string
  params: Record<string, string>
  setRights: boolean
  failure: string | null
}

/**
 * The tool-use notice of the open conversation: set by `chat-tool-availability` the first time
 * a state shows up for a conversation, hidden when another conversation is open, closed by
 * `dismiss`. A delegate turn asks for the delegate's MCP rights first (spec 021 US6): a
 * registered delegate has holzi's tools and needs no notice; otherwise the notice offers
 * "Rechte festlegen", which opens the registration dialog through a guardrail action.
 */
export function useToolNotice(activeThreadId: Ref<string | null>) {
  const { errString } = useErrorString()
  const current = ref<{
    threadId: string
    notice: ToolNotice
    kind?: AgentKind
  } | null>(null)

  function show(
    threadId: string,
    key: string,
    delegate?: { kind: AgentKind; status: DelegateStatus },
  ) {
    current.value = {
      threadId,
      kind: delegate?.kind,
      notice: {
        key,
        params: { name: delegate ? (DELEGATE_NAMES[delegate.kind] ?? '') : '' },
        setRights: delegate ? canSetRights(delegate.status) : false,
        failure: null,
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
    show(threadId, key, status ? { kind, status } : undefined)
  }

  function handleToolAvailability(e: ToolAvailabilityEvent) {
    if (e.state === 'delegate' && e.delegateKind) {
      void showDelegateAsync(e.threadId, e.delegateKind)
      return
    }
    const key = noticeKey(e.state)
    if (key && notices.show(e.threadId, e.state)) show(e.threadId, key)
  }

  /** The notice to show now, if any. */
  const toolNotice = computed(() =>
    current.value && current.value.threadId === activeThreadId.value
      ? current.value.notice
      : null,
  )

  function dismissToolNotice() {
    current.value = null
  }

  /** "Rechte festlegen": the registration dialog for the delegate of the notice. */
  async function setDelegateRightsAsync() {
    const shown = current.value
    if (!shown?.kind) return
    const kind = shown.kind
    try {
      const register = useActionOrThrow('settings.agents.mcp.delegate.register')
      const result = (await register({ kind })) as DelegateRegistration
      if (current.value !== shown) return
      if (result === 'registered') current.value = null
      else if (result === 'off')
        show(shown.threadId, 'chat.toolNotice.delegateOff', {
          kind,
          status: 'off',
        })
    } catch (error) {
      if (current.value === shown)
        current.value = {
          ...shown,
          notice: { ...shown.notice, failure: errString(error) },
        }
    }
  }

  return {
    toolNotice,
    handleToolAvailability,
    dismissToolNotice,
    setDelegateRightsAsync,
  }
}
