// Chat action catalog (spec 020-tab-navigation, T049, contracts/wm-actions.md §1). Tab-bound
// actions run in the mounted chat (`composables/useChatTab.ts` registers them; the runner opens
// the chat first if needed). Model, reasoning and voice preferences live in global stores and are
// global actions (`stores/chatActionHandlers.ts`). Approvals, the permission mode and model
// integrity overrides are guardrails: never callable by agents (FR-032).
//
// Exempt from the catalog (`action-exempt:` at the control): draft editing (typing, attachments,
// voice recording as an input method) and view state (dialogs, accordions, editing mode, error
// dismissal, HuggingFace browsing and previews).
import type { JsonSchema, ActionDefinition } from './types.ts'

const THREAD_ID: JsonSchema = {
  type: 'string',
  description: 'Conversation id (see chat.conversations.list).',
}
const DONE: JsonSchema = {
  type: 'object',
  properties: { done: { type: 'boolean' } },
}
const ANY_OBJECT: JsonSchema = { type: 'object' }
const NO_INPUT: JsonSchema = { type: 'object', properties: {} }

type Spec = Pick<ActionDefinition, 'id' | 'description' | 'scope' | 'effect'> &
  Partial<
    Pick<
      ActionDefinition,
      'input' | 'result' | 'agentCallable' | 'builtinAgentCallable'
    >
  >

function inChat(spec: Spec): ActionDefinition {
  return {
    titleKey: `actions.${spec.id}`,
    input: NO_INPUT,
    result: DONE,
    target: 'none',
    agentCallable: spec.scope !== 'guardrails',
    binding: 'tab',
    appId: 'system.chat',
    ...spec,
  }
}

function global(spec: Spec): ActionDefinition {
  return {
    titleKey: `actions.${spec.id}`,
    input: NO_INPUT,
    result: DONE,
    target: 'none',
    agentCallable: spec.scope !== 'guardrails',
    binding: 'global',
    ...spec,
  }
}

export const CHAT_ACTIONS: readonly ActionDefinition[] = [
  inChat({
    id: 'chat.conversation.new',
    description: 'Start a new conversation in the chat.',
    scope: 'chat.write',
    effect: 'write',
  }),
  inChat({
    id: 'chat.conversation.open',
    description: 'Open an existing conversation in the chat.',
    input: {
      type: 'object',
      properties: { threadId: THREAD_ID },
      required: ['threadId'],
    },
    scope: 'chat.read',
    effect: 'write',
  }),
  inChat({
    id: 'chat.conversation.rename',
    description: 'Rename a conversation.',
    input: {
      type: 'object',
      properties: { threadId: THREAD_ID, title: { type: 'string' } },
      required: ['threadId', 'title'],
    },
    scope: 'chat.write',
    effect: 'write',
  }),
  inChat({
    id: 'chat.conversation.delete',
    description:
      'Delete a conversation with all its messages; a running reply in it is cancelled first.',
    input: {
      type: 'object',
      properties: { threadId: THREAD_ID },
      required: ['threadId'],
    },
    scope: 'chat.write',
    effect: 'destructive',
  }),
  // Spec 072: contact and group conversations. Locked for agents (`guardrails`): they never reach
  // contact conversations through these actions; their own, confirmed access comes with spec 072
  // US7 (FR-070, FR-071).
  inChat({
    id: 'chat.contact.open',
    description: 'Open an existing conversation with contacts in the chat.',
    input: {
      type: 'object',
      properties: { threadId: THREAD_ID },
      required: ['threadId'],
    },
    scope: 'guardrails',
    effect: 'write',
    agentCallable: false,
  }),
  inChat({
    id: 'chat.contact.openDirect',
    description:
      'Open the conversation with a contact, starting it the first time.',
    input: {
      type: 'object',
      properties: {
        contactId: { type: 'string', description: 'Contact id.' },
      },
      required: ['contactId'],
    },
    scope: 'guardrails',
    effect: 'write',
    agentCallable: false,
  }),
  inChat({
    id: 'chat.notes.open',
    description:
      'Open the notes to oneself ("Ich"), starting them the first time.',
    input: { type: 'object', properties: {} },
    scope: 'guardrails',
    effect: 'write',
    agentCallable: false,
  }),
  inChat({
    id: 'chat.conversation.mute',
    description: 'Mute or unmute a conversation with contacts.',
    input: {
      type: 'object',
      properties: { threadId: THREAD_ID, muted: { type: 'boolean' } },
      required: ['threadId', 'muted'],
    },
    scope: 'guardrails',
    effect: 'write',
    agentCallable: false,
  }),
  // Spec 072 US2: messages to contacts. Locked for agents like the actions above: an agent sends
  // only after the user confirmed it, which comes with spec 072 US7 (FR-071).
  inChat({
    id: 'chat.peer.send',
    description: 'Send a text message in a conversation with contacts.',
    input: {
      type: 'object',
      properties: {
        threadId: THREAD_ID,
        text: { type: 'string', description: 'At most 4,000 characters.' },
        idempotencyKey: {
          type: 'string',
          description: 'A UUID made once per message; a retry sends it once.',
        },
      },
      required: ['threadId', 'text', 'idempotencyKey'],
    },
    result: {
      type: 'object',
      properties: { messageId: { type: 'string' } },
    },
    scope: 'guardrails',
    effect: 'write',
    agentCallable: false,
  }),
  inChat({
    id: 'chat.peer.resend',
    description: 'Send a message again whose delivery failed.',
    input: {
      type: 'object',
      properties: {
        messageId: { type: 'string', description: 'Message id.' },
      },
      required: ['messageId'],
    },
    scope: 'guardrails',
    effect: 'write',
    agentCallable: false,
  }),
  inChat({
    id: 'chat.conversations.list',
    description: 'List all conversations with id, title and creation time.',
    result: ANY_OBJECT,
    scope: 'chat.read',
    effect: 'read',
  }),
  inChat({
    id: 'chat.messages.list',
    description:
      'List the messages of a conversation: for a person with the text, for an agent only id, role, time and status (never the text).',
    input: {
      type: 'object',
      properties: { threadId: THREAD_ID },
      required: ['threadId'],
    },
    result: ANY_OBJECT,
    scope: 'chat.read',
    effect: 'read',
  }),
  inChat({
    id: 'chat.message.send',
    description:
      'Send a message in the open conversation (a new conversation if none is open) and start a reply.',
    input: {
      type: 'object',
      properties: { text: { type: 'string' } },
      required: ['text'],
    },
    scope: 'chat.write',
    effect: 'write',
    builtinAgentCallable: false,
  }),
  inChat({
    id: 'chat.message.retry',
    description: 'Retry the last message that failed to send.',
    scope: 'chat.write',
    effect: 'write',
    builtinAgentCallable: false,
  }),
  inChat({
    id: 'chat.reply.cancel',
    description: 'Cancel the running reply.',
    scope: 'chat.write',
    effect: 'write',
    builtinAgentCallable: false,
  }),
  inChat({
    id: 'chat.approval.decide',
    description: 'Allow or deny a pending tool permission request. User only.',
    input: {
      type: 'object',
      properties: {
        requestId: { type: 'string' },
        decision: { type: 'string', enum: ['allow', 'deny'] },
      },
      required: ['requestId', 'decision'],
    },
    scope: 'guardrails',
    effect: 'write',
  }),
  inChat({
    id: 'chat.choice.answer',
    description:
      "Answer the agent's pending question (spec 046): one of its options, own words, or cancel. User only.",
    input: {
      type: 'object',
      properties: {
        requestId: { type: 'string' },
        kind: { type: 'string', enum: ['option', 'text', 'cancel'] },
        value: {
          type: 'string',
          description: "The option's value or the own words.",
        },
      },
      required: ['requestId', 'kind'],
    },
    scope: 'guardrails',
    effect: 'write',
  }),
  inChat({
    id: 'chat.permissionMode.set',
    description:
      'Set the tool permission mode (manual, auto, plan). User only.',
    input: {
      type: 'object',
      properties: {
        mode: { type: 'string', enum: ['manual', 'auto', 'plan'] },
      },
      required: ['mode'],
    },
    scope: 'guardrails',
    effect: 'write',
  }),
]

export const CHAT_MODEL_ACTIONS: readonly ActionDefinition[] = [
  global({
    id: 'chat.model.select',
    description: 'Choose and load the model the chat answers with.',
    input: {
      type: 'object',
      properties: { modelId: { type: 'string' } },
      required: ['modelId'],
    },
    scope: 'settings.models',
    effect: 'write',
  }),
  global({
    id: 'chat.reasoning.set',
    description:
      "Set the reasoning effort for the active model; omit 'level' for automatic.",
    input: {
      type: 'object',
      properties: { level: { type: 'string' } },
    },
    scope: 'chat.write',
    effect: 'write',
  }),
  global({
    id: 'chat.model.retryLoad',
    description: 'Retry loading the active model after a failure.',
    scope: 'settings.models',
    effect: 'write',
  }),
  global({
    id: 'chat.model.downloadRecommended',
    description: 'Download a model from the recommended catalog.',
    input: {
      type: 'object',
      properties: { entryId: { type: 'string' } },
      required: ['entryId'],
    },
    scope: 'settings.models',
    effect: 'write',
  }),
  global({
    id: 'chat.modelIntegrity.decide',
    description:
      'Resolve a failed model integrity check: load anyway, repair the source, or choose another model. User only.',
    input: {
      type: 'object',
      properties: {
        decision: {
          type: 'string',
          enum: ['loadUntrusted', 'repairSource', 'chooseOther'],
        },
      },
      required: ['decision'],
    },
    scope: 'guardrails',
    effect: 'write',
  }),
  global({
    id: 'chat.voice.setAutoSend',
    description:
      'Send dictated text automatically when recording stops, or not.',
    input: {
      type: 'object',
      properties: { enabled: { type: 'boolean' } },
      required: ['enabled'],
    },
    scope: 'chat.write',
    effect: 'write',
  }),
]
