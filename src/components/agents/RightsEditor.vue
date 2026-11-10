<script setup lang="ts">
/**
 * Every rights group of an external agent in one place (spec 021 FR-008): "holzi bedienen" per
 * topic, with the groups "Passwörter" and "Browser" (spec 065) in the same list, the extensions whose tools it sees (US4) and the group "Befehle ausführen" (US5) and the groups "Dateien" and "Kontakte" (spec 083 FR-032, FR-033) and the group "Abo-Agenten" (spec 083 FR-030).
 * Used by the registration dialog and the agent's settings page; each change emits the whole
 * draft. `resetKey` names whose rights these are, so an unfinished password tag draft does not
 * carry over to another agent.
 */
import type { RightsDraft } from '~/lib/agents/rights'

const props = defineProps<{
  modelValue: RightsDraft
  resetKey?: string
  /** Shows the rights without letting them change (an agent's profile, the registration). */
  disabled?: boolean
}>()
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
    :disabled="disabled"
    @update:model-value="set('levels', $event)"
  >
    <AgentsPasswordRightsFields
      :key="resetKey"
      :model-value="modelValue.passwords"
      :disabled="disabled"
      @update:model-value="set('passwords', $event)"
    />
    <AgentsBrowserRightsFields
      :model-value="modelValue.browser"
      :disabled="disabled"
      @update:model-value="set('browser', $event)"
    />
  </AgentsRightsFields>
  <AgentsExtensionRightsFields
    :model-value="modelValue.extensions"
    :all="modelValue.allExtensions"
    :disabled="disabled"
    @update:model-value="set('extensions', $event)"
    @update:all="set('allExtensions', $event)"
  />
  <AgentsCommandRightsFields
    :model-value="modelValue.commands"
    :disabled="disabled"
    @update:model-value="set('commands', $event)"
  />
  <AgentsFilesRightsFields
    :model-value="modelValue.files"
    :disabled="disabled"
    @update:model-value="set('files', $event)"
  />
  <AgentsContactsRightsFields
    :model-value="modelValue.contacts"
    :disabled="disabled"
    @update:model-value="set('contacts', $event)"
  />
  <AgentsDelegateRightsFields
    :model-value="modelValue.delegate"
    :disabled="disabled"
    @update:model-value="set('delegate', $event)"
  />
</template>
