// The actions of the voice assistant (spec 066 T049, FR-011, FR-014, US9): talk, mute, end the
// conversation and show it. Guardrails: they turn the microphone on or decide when haex speaks,
// which is the user's choice only, so no agent can call them. Handlers are in
// `stores/assistantActionHandlers.ts`. Pure, relative `.ts` imports only.
import type { ActionDefinition } from './types.ts'

const DONE = {
  type: 'object',
  properties: { done: { type: 'boolean' } },
  required: ['done'],
} as const

/** The fields every assistant action shares. */
const GUARDRAIL = {
  result: DONE,
  target: 'none',
  scope: 'guardrails',
  agentCallable: false,
  binding: 'global',
  effect: 'write',
} as const

export const ASSISTANT_ACTIONS: readonly ActionDefinition[] = [
  {
    id: 'assistant.talk',
    titleKey: 'actions.assistant.talk',
    description:
      'Start talking to the voice assistant, or interrupt it while it speaks. Until the wake word arrives, only with the preview setting on. User only.',
    input: { type: 'object', properties: {} },
    ...GUARDRAIL,
    defaultKeys: {
      default: ['Ctrl+Shift+Space'],
      mac: ['Shift+Meta+Space'],
    },
  },
  {
    id: 'assistant.mute.set',
    titleKey: 'actions.assistant.mute.set',
    description:
      'Mute or unmute the voice assistant: muted, it neither listens nor speaks and answers show in the window. User only.',
    input: {
      type: 'object',
      properties: { muted: { type: 'boolean' } },
      required: ['muted'],
    },
    ...GUARDRAIL,
  },
  {
    id: 'assistant.conversation.end',
    titleKey: 'actions.assistant.conversation.end',
    description:
      'End the spoken conversation, also a round that is still running. User only.',
    input: { type: 'object', properties: {} },
    ...GUARDRAIL,
  },
  {
    id: 'assistant.conversation.show',
    titleKey: 'actions.assistant.conversation.show',
    description:
      'Open the spoken conversation (the running or the last one) in the chat. User only.',
    input: { type: 'object', properties: {} },
    ...GUARDRAIL,
  },
]
