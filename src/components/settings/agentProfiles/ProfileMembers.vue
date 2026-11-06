<script setup lang="ts">
/**
 * The agents in one agent profile (spec 083 US3, FR-016): each with its kind ("Externer Agent",
 * "Abo-Agent", "Lokaler Agent") and the device it is registered on, and a field that adds one from
 * every agent and model of the vault, each with the profile it is in now. An agent is in at most one
 * profile, so choosing one moves it. Removing a member puts it into "Standard". Both run the guardrail
 * action `settings.agents.profiles.assign`, which no agent can call.
 */
import { toast } from 'vue-sonner'
import { kindLabelKey, profileLabel } from '~/lib/agents/profiles'
import type { AgentSubjectView } from '~/types/bindings/AgentSubjectView'

const props = defineProps<{
  profileId: string
  members: AgentSubjectView[]
}>()

const { t } = useI18n()
const { errString } = useErrorString()
const store = useAgentProfilesStore()

/** The profile "Standard" has the id the store's first entry carries. */
const standardId = computed(
  () => store.profiles.find((profile) => profile.isStandard)?.id ?? '',
)
const adding = ref('')
const busy = ref(false)

function where(subject: AgentSubjectView): string {
  const parts = [t(kindLabelKey(subject.kind))]
  if (subject.device) parts.push(subject.device)
  return parts.join(' · ')
}

const candidates = computed(() => [
  { value: '', label: t('agentProfiles.members.add') },
  ...store.subjects
    .filter((subject) => subject.profileId !== props.profileId)
    .map((subject) => ({
      value: subject.subject,
      label: t('agentProfiles.members.candidate', {
        name: subject.name,
        profile: profileLabel(t, subject.profileId, subject.profileName),
      }),
    })),
])

async function moveAsync(subject: string, profileId: string) {
  if (busy.value) return
  busy.value = true
  try {
    await store.assignAsync(subject, profileId)
  } catch (error) {
    toast.error(errString(error))
  } finally {
    busy.value = false
    adding.value = ''
  }
}

function onAdd(subject: string) {
  if (!subject) return
  void moveAsync(subject, props.profileId)
}
</script>

<template>
  <SettingsGroup
    :label="t('agentProfiles.members.title')"
    data-testid="agent-profile-members"
  >
    <SettingsRow
      v-for="member in members"
      :key="member.subject"
      icon="lucide:bot"
      :title="member.name"
      :description="where(member)"
      data-testid="agent-profile-member"
      :data-subject="member.subject"
    >
      <UiButton
        v-if="standardId && profileId !== standardId"
        type="button"
        variant="outline"
        :disabled="busy"
        data-testid="agent-profile-member-remove"
        @click="moveAsync(member.subject, standardId)"
      >
        {{ t('agentProfiles.members.remove') }}
      </UiButton>
    </SettingsRow>
    <SettingsRow
      v-if="members.length === 0"
      :title="t('agentProfiles.members.none')"
    />
    <SettingsRow
      :title="t('agentProfiles.members.addLabel')"
      label-for="agent-profile-add"
    >
      <SettingsSelect
        id="agent-profile-add"
        :model-value="adding"
        class="w-72 max-w-full"
        :options="candidates"
        :disabled="busy"
        data-testid="agent-profile-add"
        @update:model-value="onAdd"
      />
    </SettingsRow>
  </SettingsGroup>
</template>
