// The settings actions for OpenAI-compatible model servers (spec 055 US5, T056): add one with its
// address and key, remove one, and state what one of its models can do. Guardrails: a key and the
// models that receive the vault's data are the user's choice only. Part of `SETTINGS_ACTIONS`;
// handlers in `stores/settingsActionHandlers.ts`. Pure, relative `.ts` imports only.
import type { ActionDefinition, JsonSchema } from './types.ts'

const DONE: JsonSchema = {
  type: 'object',
  properties: { done: { type: 'boolean' } },
}

function server(
  spec: Pick<
    ActionDefinition,
    'id' | 'description' | 'input' | 'result' | 'effect'
  >,
): ActionDefinition {
  return {
    titleKey: `actions.${spec.id}`,
    target: 'none',
    scope: 'guardrails',
    agentCallable: false,
    binding: 'global',
    ...spec,
  }
}

export const SETTINGS_SERVER_ACTIONS: readonly ActionDefinition[] = [
  server({
    id: 'settings.servers.add',
    description:
      'Add an OpenAI-compatible model server with its address and an optional key, and read its models. User only.',
    input: {
      type: 'object',
      properties: {
        name: { type: 'string' },
        baseUrl: { type: 'string' },
        apiKey: { type: 'string' },
      },
      required: ['name', 'baseUrl'],
    },
    result: {
      type: 'object',
      properties: {
        providerId: { type: 'string' },
        modelCount: {
          type: 'integer',
          description: 'Absent when the models could not be read.',
        },
        refreshError: {
          type: 'string',
          description: 'Why the models could not be read; the server is kept.',
        },
      },
    },
    effect: 'write',
  }),
  server({
    id: 'settings.servers.remove',
    description: 'Remove a model server. User only.',
    input: {
      type: 'object',
      properties: { providerId: { type: 'string' } },
      required: ['providerId'],
    },
    result: DONE,
    effect: 'destructive',
  }),
  server({
    id: 'settings.servers.setModelCapabilities',
    description:
      'State whether a model of a model server takes images and which effort levels it knows. A field left out is not stated; both left out withdraw the statement. User only.',
    input: {
      type: 'object',
      properties: {
        modelId: { type: 'string' },
        acceptsImages: { type: 'boolean' },
        effortLevels: {
          type: 'array',
          items: { type: 'string' },
          description: 'Provider-native effort ids, lowest first.',
        },
      },
      required: ['modelId'],
    },
    result: DONE,
    effect: 'write',
  }),
]
