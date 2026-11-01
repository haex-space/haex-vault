<script setup lang="ts">
/**
 * Every rights group of an external agent in one place (spec 021 FR-008): "holzi bedienen" per
 * scope, the websites of the group "Browser" (spec 065), the extensions whose tools it sees (US4)
 * and the groups "Passwörter" and "Befehle ausführen" (US5). Used by the registration dialog and
 * the agent's settings page; each change emits the whole draft. `resetKey` names whose rights
 * these are, so an unfinished password tag draft does not carry over to another agent.
 */
import type { RightsDraft } from '~/lib/agents/rights'

const props = defineProps<{ modelValue: RightsDraft; resetKey?: string }>()
const emit = defineEmits<{ 'update:modelValue': [RightsDraft] }>()

function set<Group extends keyof RightsDraft>(
  group: Group,
  value: RightsDraft[Group],
) {
  emit('update:modelValue', { ...props.modelValue, [group]: value })
}
</script>

<template>
  <AgentsRightsFields
    :model-value="modelValue.levels"
    @update:model-value="set('levels', $event)"
  />
  <AgentsBrowserRightsFields
    :model-value="modelValue.browser"
    @update:model-value="set('browser', $event)"
  />
  <AgentsExtensionRightsFields
    :model-value="modelValue.extensions"
    @update:model-value="set('extensions', $event)"
  />
  <AgentsPasswordRightsFields
    :key="resetKey"
    :model-value="modelValue.passwords"
    @update:model-value="set('passwords', $event)"
  />
  <AgentsCommandRightsFields
    :model-value="modelValue.commands"
    @update:model-value="set('commands', $event)"
  />
</template>
