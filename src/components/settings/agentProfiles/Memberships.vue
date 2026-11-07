<script setup lang="ts">
/**
 * The agent profiles one agent or model is a member of (spec 083 FR-006, FR-019, FR-023): a switch
 * per profile, so the same model can be in a sensitive and an open profile and the active profile
 * (set on KI → Agentenprofile) decides which one applies. "Standard" is not listed: every agent is
 * in it. A row names the profile that applies now and opens it. Each switch runs the guardrail
 * action `settings.agents.profiles.assign`, which no agent can call.
 */
import { toast } from 'vue-sonner'
import { profileLabel } from '~/lib/agents/profiles'

const props = defineProps<{
  /** `agent:<id>` or `model:<id>`. */
  subject: string
  /** Explains who the subject is, under the title. */
  description?: string
}>()

const { t } = useI18n()
const { errString } = useErrorString()
const store = useAgentProfilesStore()

const entry = computed(() =>
  store.subjects.find((subject) => subject.subject === props.subject),
)
const own = computed(() =>
  store.profiles.filter((profile) => !profile.isStandard),
)
const applying = computed(() => entry.value?.profileId ?? '')
const applyingName = computed(() =>
  entry.value
    ? profileLabel(t, entry.value.profileId, entry.value.profileName)
    : '',
)
const busy = ref(false)

function isMember(profileId: string): boolean {
  return (
    entry.value?.profiles.some((profile) => profile.id === profileId) ?? false
  )
}

async function setAsync(profileId: string, member: boolean) {
  if (busy.value) return
  busy.value = true
  try {
    await store.setMemberAsync(props.subject, profileId, member)
  } catch (error) {
    toast.error(errString(error))
  } finally {
    busy.value = false
  }
}
</script>

<template>
  <SettingsGroup
    :label="t('agentProfiles.memberships.title')"
    data-testid="agent-memberships"
  >
    <SettingsRow
      v-if="description"
      :title="t('agentProfiles.memberships.standard')"
      :description="description"
    />
    <SettingsRow
      v-for="profile in own"
      :key="profile.id"
      :title="profileLabel(t, profile.id, profile.name)"
      :description="
        applying === profile.id
          ? t('agentProfiles.memberships.applies')
          : profile.isActive
            ? t('agentProfiles.memberships.activeNotMember')
            : undefined
      "
      :label-for="`agent-membership-${profile.id}`"
    >
      <ShadcnSwitch
        :id="`agent-membership-${profile.id}`"
        :model-value="isMember(profile.id)"
        :disabled="busy"
        data-testid="agent-membership-switch"
        :data-profile="profile.id"
        @update:model-value="setAsync(profile.id, $event === true)"
      />
    </SettingsRow>
    <SettingsRow
      v-if="own.length === 0"
      :title="t('agentProfiles.memberships.none')"
      :description="t('agentProfiles.memberships.noneHint')"
      to="/ai/profiles"
    />
    <SettingsRow
      v-if="applying"
      icon="lucide:users"
      :title="t('agentProfiles.agent.open', { name: applyingName })"
      :description="t('agentProfiles.agent.openHint')"
      :to="`/ai/profiles/${applying}`"
      data-testid="agent-profile-link"
    />
  </SettingsGroup>
</template>
