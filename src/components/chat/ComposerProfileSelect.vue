<script setup lang="ts">
/**
 * The agent profile select of the composer's settings (spec 083 FR-006b): a quick way to change
 * the active profile right where the model is chosen. The profile decides which models the chat
 * offers and what the agent may do. The popover of a wide screen and the drawer of a small one
 * both show it. It reads and changes the vault's active profile (the same one KI → Agentenprofile
 * sets) and is not shown while "Standard" is the only profile, because there is nothing to
 * choose.
 */
import { computed } from 'vue'
import { profileLabel } from '~/lib/agents/profiles'

defineProps<{
  id: string
  disabled?: boolean
}>()

const { t } = useI18n()
const store = useAgentProfilesStore()
const activation = useAgentProfileActivation()

const options = computed(() =>
  store.profiles.map((profile) => ({
    value: profile.id,
    label: profileLabel(t, profile.id, profile.name),
  })),
)
const activeId = computed(
  () => store.profiles.find((profile) => profile.isActive)?.id ?? '',
)

function update(value: unknown) {
  if (typeof value === 'string') void activation.activateAsync(value)
}
</script>

<template>
  <div v-if="store.profiles.length > 1" class="mb-4">
    <label
      :for="id"
      class="mb-1 block text-xs font-medium text-muted-foreground"
    >
      {{ t('chat.composer.settingsPopover.profileLabel') }}
    </label>
    <UiSelect
      :id="id"
      :model-value="activeId"
      :options="options"
      :aria-label="t('chat.composer.settingsPopover.profileLabel')"
      :disabled="disabled"
      data-testid="chat-profile-select"
      @update:model-value="update"
    />
  </div>
</template>
