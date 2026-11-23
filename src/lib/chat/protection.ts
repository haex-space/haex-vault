/**
 * How the conversation view names a protection state (spec 095 US4, FR-013): a label, an icon and
 * an explanation per state, and the two limits every explanation ends with: what Nostr-Relays see
 * and can do (FR-014), and that the kept history is outside this protection (FR-024). The texts
 * are i18n keys under `chat.peer.protection`; `scripts/check-chat-protection.ts` checks that every
 * state has them.
 */
import type { ChatPeerProtectionState } from '../../types/bindings/ChatPeerProtectionState.ts'

/** Every state Rust answers, strongest first. */
export const PROTECTION_STATES = [
  'protected',
  'renewalOutstanding',
  'recovering',
  'waitingForKeys',
] as const satisfies readonly ChatPeerProtectionState[]

export interface ProtectionView {
  /** i18n key of the short name shown in the header. */
  label: string
  icon: string
  /** i18n key of the explanation in plain words. */
  explanation: string
  /** The state is weaker than `protected`: shown in the warning colour. */
  warning: boolean
}

export const PROTECTION_VIEWS: Record<ChatPeerProtectionState, ProtectionView> =
  {
    protected: {
      label: 'chat.peer.protection.state.protected',
      icon: 'lucide:shield-check',
      explanation: 'chat.peer.protection.explain.protected',
      warning: false,
    },
    renewalOutstanding: {
      label: 'chat.peer.protection.state.renewalOutstanding',
      icon: 'lucide:shield-alert',
      explanation: 'chat.peer.protection.explain.renewalOutstanding',
      warning: true,
    },
    recovering: {
      label: 'chat.peer.protection.state.recovering',
      icon: 'lucide:refresh-cw',
      explanation: 'chat.peer.protection.explain.recovering',
      warning: true,
    },
    waitingForKeys: {
      label: 'chat.peer.protection.state.waitingForKeys',
      icon: 'lucide:key-round',
      explanation: 'chat.peer.protection.explain.waitingForKeys',
      warning: true,
    },
  }

/** i18n keys shown after every explanation (FR-014, FR-024). */
export const PROTECTION_LIMITS = [
  'chat.peer.protection.relays',
  'chat.peer.protection.history',
] as const
