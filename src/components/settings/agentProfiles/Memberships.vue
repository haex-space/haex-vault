<script setup lang="ts">
/**
 * The agent profiles one agent or model is a member of (spec 083 FR-006, FR-019, FR-023): a switch
 * per profile, so the same model can be in a sensitive and an open profile and the active profile
 * (set on KI → Agentenprofile) decides which one applies. Every agent is in "Standard" and has no
 * switch for it; a model has one, because the chat offers only the models of the active profile,
 * "Standard" included (FR-006b). A row names the profile that applies now and opens it. Each
 * switch runs the guardrail action `settings.agents.profiles.assign`, which no agent can call.
 */
import { toast } from 'vue-sonner'
import { profileLabel, STANDARD_PROFILE_ID } from '~/lib/agents/profiles'

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
const isModel = computed(
  () =>
    props.subject.startsWith('model:') || props.subject.startsWith('provider:'),
)
const isProvider = computed(() => props.subject.startsWith('provider:'))
const standardActive = computed(
  () => store.profiles.find((profile) => profile.isActive)?.isStandard ?? true,
)
const busy = ref(false)

function isMember(profileId: string): boolean {
  return (
    entry.value?.profiles.some((profile) => profile.id === profileId) ?? false
  )
}

/** In only because the provider of the model is: the provider's page takes it back. */
function viaProvider(profileId: string): boolean {
  return (
    !isMember(profileId) &&
    (entry.value?.inherited.some((profile) => profile.id === profileId) ??
      false)
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
      v-if="isModel"
      :title="profileLabel(t, STANDARD_PROFILE_ID, '')"
      :description="
        isProvider
          ? t('agentProfiles.provider.standardOffered')
          : standardActive
            ? t('agentProfiles.memberships.standardOfferedActive')
            : t('agentProfiles.memberships.standardOffered')
      "
      label-for="agent-membership-standard"
    >
      <ShadcnSwitch
        id="agent-membership-standard"
        :model-value="entry?.inStandard ?? true"
        :disabled="busy"
        data-testid="agent-membership-switch"
        :data-profile="STANDARD_PROFILE_ID"
        @update:model-value="setAsync(STANDARD_PROFILE_ID, $event === true)"
      />
    </SettingsRow>
    <SettingsRow
      v-else-if="description"
      :title="t('agentProfiles.memberships.standard')"
      :description="description"
    />
    <SettingsRow
      v-for="profile in own"
      :key="profile.id"
      :title="profileLabel(t, profile.id, profile.name)"
      :description="
        viaProvider(profile.id)
          ? t('agentProfiles.provider.viaProvider')
          : applying === profile.id
            ? t('agentProfiles.memberships.applies')
            : profile.isActive
              ? t('agentProfiles.memberships.activeNotMember')
              : undefined
      "
      :label-for="`agent-membership-${profile.id}`"
    >
      <ShadcnSwitch
        :id="`agent-membership-${profile.id}`"
        :model-value="isMember(profile.id) || viaProvider(profile.id)"
        :disabled="busy || viaProvider(profile.id)"
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
