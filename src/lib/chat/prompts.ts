// What a chat turn waits on the user for (spec 032 approvals, spec 046 questions): the queue the
// chat shows oldest first, and the wire shapes of a question and its answer. Types only.

import type { ToolOrigin } from './toolOrigin.ts'

export type RiskClass = 'safe' | 'change' | 'risky'

/** A tool call waiting for the user's approval. */
export interface PendingApproval {
  kind: 'approval'
  requestId: string
  toolName: string
  toolInput: unknown
  riskClass: RiskClass
  /** `action` tools are worded in plain language; others keep the raw layout. */
  toolSource?: 'mcp' | 'cli' | 'action' | 'haextension'
  /** An extension's tool (spec 018): named by its extension and title. */
  toolOrigin?: ToolOrigin
}

/** One proposed answer of a choice (spec 046). */
export interface ChoiceOptionWire {
  value: string
  label: string
  /** Why it cannot be picked right now; shown disabled. */
  unavailable?: string
}

/** Payload of `chat-choice-request` (spec 046, contracts/choice-contract.md). Answered via
 * `respond_choice`. */
export interface ChoiceRequestEvent {
  requestId: string
  threadId: string
  toolName: string
  /** The agent's own question (`ask_user`); `null` for an action, worded by the chat. */
  question: string | null
  /** The action's input field the answer goes into; `null` for `ask_user`. */
  field: string | null
  /** What was asked for, e.g. the app name; empty for `ask_user`. */
  value: string
  options: ChoiceOptionWire[]
}

/** A question of the agent waiting for the user's answer. */
export interface PendingChoice extends Omit<ChoiceRequestEvent, 'threadId'> {
  kind: 'choice'
}

export type ChoiceAnswer =
  | { kind: 'option'; value: string }
  | { kind: 'text'; text: string }
  | { kind: 'cancel' }

/** What a turn waits on the user for, oldest first; the chat shows only the first. */
export type PendingPrompt = PendingApproval | PendingChoice
