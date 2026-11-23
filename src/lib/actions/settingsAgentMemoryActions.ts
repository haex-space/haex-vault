// The settings actions of the agents' shared memory (spec 097, contracts "Tauri commands"): the
// user adds, changes and removes pages in "KI → Gedächtnis". Guardrails: what the memory holds is
// read by every agent that may read it, so writing it from the window is the user's choice only, and
// no agent can call them; agents write through their own actions under the rules of spec 097.
// Listed in `catalog.ts`; the handlers are in `stores/settingsActionHandlers.ts`. Pure, relative
// `.ts` imports only.
import type { ActionDefinition } from './types.ts'

/** The fields every guardrail on the memory shares. */
const GUARDRAIL = {
  target: 'none',
  scope: 'guardrails',
  agentCallable: false,
  binding: 'global',
} as const

const PAGE_ID = {
  type: 'string',
  description: 'The id of a page of the memory.',
} as const

const PAGE_FIELDS = {
  title: { type: 'string', description: 'One line, at most 80 characters.' },
  description: {
    type: 'string',
    description: 'One line, at most 200 characters.',
  },
  body: {
    type: 'string',
    description:
      'Markdown, at most 10,000 characters; [[Title]] points to another page.',
  },
} as const

export const AGENT_MEMORY_ACTIONS: readonly ActionDefinition[] = [
  {
    id: 'settings.agentMemory.add',
    titleKey: 'actions.settings.agentMemory.add',
    description:
      'Add a page to the shared memory of the agents, written by the user. Answers the page.',
    input: {
      type: 'object',
      properties: PAGE_FIELDS,
      required: ['title', 'description', 'body'],
    },
    effect: 'write',
    result: { type: 'object' },
    ...GUARDRAIL,
  },
  {
    id: 'settings.agentMemory.change',
    titleKey: 'actions.settings.agentMemory.change',
    description:
      'Replace title, description and text of a page of the shared memory. Answers the page.',
    input: {
      type: 'object',
      properties: { id: PAGE_ID, ...PAGE_FIELDS },
      required: ['id', 'title', 'description', 'body'],
    },
    effect: 'write',
    result: { type: 'object' },
    ...GUARDRAIL,
  },
  {
    id: 'settings.agentMemory.remove',
    titleKey: 'actions.settings.agentMemory.remove',
    description: 'Remove a page of the shared memory on every device.',
    input: {
      type: 'object',
      properties: { id: PAGE_ID },
      required: ['id'],
    },
    effect: 'destructive',
    result: { type: 'object', nullable: true },
    ...GUARDRAIL,
  },
]
