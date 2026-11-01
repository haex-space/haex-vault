// The notice about a model's tool use, shown once per conversation and state (spec 032 US4,
// FR-016/017/023, research R11). Pure, so the Node check scripts can load it.
import type { DelegateStatus } from '../../types/bindings/DelegateStatus.ts'

/** What the backend decided for a turn (`chat-tool-availability`). */
export type ToolAvailability =
  'offered' | 'offeredUnverified' | 'unsupported' | 'delegate'

/** The i18n key of the notice for a state, or `null` when the state needs none. */
export function noticeKey(state: ToolAvailability): string | null {
  switch (state) {
    case 'offeredUnverified':
      return 'chat.toolNotice.unverified'
    case 'unsupported':
      return 'chat.toolNotice.unsupported'
    case 'delegate':
      return 'chat.toolNotice.delegate'
    case 'offered':
      return null
  }
}

/** The notice of a delegate turn by its MCP status; a registered delegate needs none. */
export function delegateNoticeKey(status: DelegateStatus): string | null {
  switch (status) {
    case 'off':
      return 'chat.toolNotice.delegateOff'
    case 'unregistered':
      return 'chat.toolNotice.delegateUnregistered'
    case 'expired':
      return 'chat.toolNotice.delegateExpired'
    case 'registered':
      return null
  }
}

/** Whether the notice of a delegate offers "Rechte festlegen". */
export function canSetRights(status: DelegateStatus): boolean {
  return status === 'unregistered' || status === 'expired'
}

/**
 * Remembers which notices were already shown. `show(threadId, state)` is true the first time a
 * state is seen for a conversation, and never for a state without a notice.
 *
 * ponytail: the memory lives in the app session only, so a restart shows a notice again.
 * Ceiling: one repeated, dismissible line per conversation after a restart. Upgrade path: a
 * system row in `chat_messages`, which would sync and needs a localized marker (research R11).
 */
export function createToolNotices() {
  const seen = new Set<string>()
  return {
    show(threadId: string, state: ToolAvailability): boolean {
      if (noticeKey(state) === null) return false
      const key = `${threadId}:${state}`
      if (seen.has(key)) return false
      seen.add(key)
      return true
    },
  }
}
