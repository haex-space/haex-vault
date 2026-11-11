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
      // Delegate turns need their MCP status to decide which notice, if any, applies.
      return null
    case 'offered':
      return null
  }
}

/** The notice of a delegate turn by its MCP status; with the access on, the agent profile of its
 * model decides what it may do (spec 083 FR-029) and there is nothing to say. */
export function delegateNoticeKey(status: DelegateStatus): string | null {
  switch (status) {
    case 'off':
      return 'chat.toolNotice.delegateOff'
    case 'available':
      return null
  }
}

/** What an agent profile does not give the model of a turn (Rust `profile_gate::Missing`). It names
 * no level: it never says the model may only read. */
export type ProfileMissing =
  | { kind: 'browser' }
  | { kind: 'passwords' }
  | { kind: 'topic'; topic: string }
  | { kind: 'topicWrite'; topic: string }
  | { kind: 'commands' }
  | { kind: 'extension'; name: string }

/** The payload of `chat-profile-restricted` (spec 083 US5): a call the agent profile of the
 * chat's model does not permit was turned away. */
export interface ProfileRestrictedEvent {
  threadId: string
  profileId: string
  profileName: string
  missing: ProfileMissing
}

/** The payload of `chat-compaction` (spec 086): the built-in agent is summarising the older messages of a
 * conversation so that the turn fits the model's context, has done so, could not, or was stopped with the turn. */
export interface CompactionEvent {
  threadId: string
  state: 'summarising' | 'done' | 'failed' | 'cancelled'
}

/** The i18n key of the notice for what the profile does not give, one per kind. */
export function restrictedKey(missing: ProfileMissing): string {
  return `chat.agentProfile.restricted.${missing.kind}`
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
    /** The notice about an agent profile that turned a call away: once per conversation, profile,
     * and missing right. */
    showRestricted(
      threadId: string,
      profileId: string,
      missing: ProfileMissing,
    ): boolean {
      const detail =
        'topic' in missing
          ? missing.topic
          : 'name' in missing
            ? missing.name
            : ''
      const key = `${threadId}:profile:${profileId}:${missing.kind}:${detail}`
      if (seen.has(key)) return false
      seen.add(key)
      return true
    },
    show(threadId: string, state: ToolAvailability): boolean {
      if (noticeKey(state) === null) return false
      const key = `${threadId}:${state}`
      if (seen.has(key)) return false
      seen.add(key)
      return true
    },
  }
}
