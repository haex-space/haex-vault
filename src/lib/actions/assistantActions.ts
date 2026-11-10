// The action of the voice assistant (spec 066 T049, FR-001; operator 2026-11-02: one on/off
// toggle in dock and launcher, no further buttons). Guardrail: it turns the microphone on, which
// is the user's choice only, so no agent can call it. The handler is in
// `stores/assistantActionHandlers.ts`. Pure, relative `.ts` imports only.
import type { ActionDefinition } from './types.ts'

export const ASSISTANT_ACTIONS: readonly ActionDefinition[] = [
  {
    id: 'assistant.toggle',
    titleKey: 'actions.assistant.toggle',
    description:
      'Switch the voice assistant on or off. On, it waits for the wake word; without the wake word model it starts a conversation at once. When something it needs is missing, it stays off and the settings open where that is set up. Off ends a conversation and turns the microphone off. User only.',
    input: { type: 'object', properties: {} },
    result: {
      type: 'object',
      properties: { enabled: { type: 'boolean' } },
      required: ['enabled'],
    },
    target: 'none',
    scope: 'guardrails',
    agentCallable: false,
    binding: 'global',
    effect: 'write',
  },
]
