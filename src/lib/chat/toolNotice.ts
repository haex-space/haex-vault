// The notice about a model's tool use, shown once per conversation and state (spec 032 US4,
// FR-016/017/023, research R11). Pure, so the Node check scripts can load it.
import type { AgentKind } from '../../types/bindings/AgentKind.ts'

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
      // A delegate has haex-vault's tools on every start, as far as the agent profile of its model
      // allows (spec 102 FR-004, spec 083 FR-029); a start without them sends
      // `chat-delegate-tools-unavailable`.
      return null
    case 'offered':
      return null
  }
}

/** The payload of `chat-delegate-tools-unavailable` (spec 102 FR-012): a delegate's start has no
 * haex-vault tools, and why. */
export interface DelegateToolsUnavailableEvent {
  threadId: string
  delegateKind: AgentKind
  reason: string
}

/** The i18n key of the notice about a delegate start without haex-vault's tools. */
export const DELEGATE_UNAVAILABLE_KEY = 'chat.toolNotice.delegateUnavailable'

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

/** The payload of `chat-delegate-own-web` (spec 102 FR-013): a delegate used its own web access,
 * which its program cannot block (Abacus' server-side web search and page fetch). */
export interface DelegateOwnWebEvent {
  threadId: string
  delegateKind: AgentKind
}

/** The i18n key of the notice about a delegate's own web access. */
export const DELEGATE_OWN_WEB_KEY = 'chat.toolNotice.delegateOwnWeb'

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
    /** The notice about a delegate start without haex-vault's tools: once per conversation and
     * reason (spec 102 FR-012). */
    showUnavailable(threadId: string, reason: string): boolean {
      const key = `${threadId}:unavailable:${reason}`
      if (seen.has(key)) return false
      seen.add(key)
      return true
    },
    /** The notice about a delegate's own web access: once per conversation (spec 102 FR-013). */
    showOwnWeb(threadId: string): boolean {
      const key = `${threadId}:ownWeb`
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
