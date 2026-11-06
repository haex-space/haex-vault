<script setup lang="ts">
/**
 * One model of the local agent (spec 083 US5, FR-008): the in-app chat agent has no registration,
 * its subject is the model that answers. Shows the model's agent profile as a field, the rights
 * that profile gives it (read only; changed on the profile's page), and a note for a model that is
 * assigned but not offered any more, which can be moved back to "Standard". Choosing another
 * agent profile applies on selection without a save button (023 FR-021) and holds on every device,
 * because the assignment is part of the vault. Runs the guardrail action
 * `settings.agents.profiles.assign`, which no agent can call.
 */
import { toast } from 'vue-sonner'
import { invoke } from '@tauri-apps/api/core'
import { kindLabelKey, profileLabel } from '~/lib/agents/profiles'
import { rightsDraftOf, type RightsDraft } from '~/lib/agents/rights'
import type { AgentProfileDetail } from '~/types/bindings/AgentProfileDetail'

const { t } = useI18n()
const { errString } = useErrorString()
const router = useTabRouter()
const store = useAgentProfilesStore()

const modelId = computed(() => String(router.route.params.modelId ?? ''))
const subjectKey = computed(() => `model:${modelId.value}`)
const model = computed(() =>
  store.subjects.find((subject) => subject.subject === subjectKey.value),
)
const draft = ref<RightsDraft>(rightsDraftOf(null))
const loaded = ref(false)

const profileOptions = computed(() =>
  store.profiles.map((profile) => ({
    value: profile.id,
    label: profileLabel(t, profile.id, profile.name),
  })),
)
const profileName = computed(() =>
  model.value
    ? profileLabel(t, model.value.profileId, model.value.profileName)
    : '',
)

/** The rights of the model's profile, for the read only editor. */
async function loadRightsAsync() {
  const current = model.value
  if (!current) return
  try {
    const detail = await invoke<AgentProfileDetail>('agent_profile_get', {
      args: { id: current.profileId },
    })
    draft.value = rightsDraftOf(detail.rights)
  } catch (error) {
    toast.error(errString(error))
  }
}

async function reloadAsync() {
  try {
    await store.reloadAsync()
  } catch (error) {
    toast.error(errString(error))
  } finally {
    loaded.value = true
  }
  await loadRightsAsync()
}

async function onProfile(profileId: string) {
  if (!model.value || profileId === model.value.profileId) return
  try {
    await store.assignAsync(subjectKey.value, profileId)
  } catch (error) {
    toast.error(errString(error))
  }
  await reloadAsync()
}

onMounted(reloadAsync)
watch(modelId, reloadAsync)
watch(
  () => model.value?.profileId,
  () => void loadRightsAsync(),
)
</script>

<template>
  <section class="flex flex-col gap-3" data-testid="agent-model-detail">
    <p
      v-if="loaded && !model"
      class="px-1 text-sm text-muted-foreground"
      role="status"
    >
      {{ t('agentProfiles.model.gone') }}
    </p>
    <template v-if="model">
      <SettingsGroup>
        <SettingsRow
          :title="model.name"
          :description="t(kindLabelKey(model.kind))"
        />
        <SettingsRow
          v-if="model.missing"
          :title="t('agentProfiles.model.missing')"
          :description="t('agentProfiles.model.missingHint')"
          data-testid="agent-model-missing"
        />
      </SettingsGroup>

      <SettingsGroup>
        <SettingsRow
          :title="t('agentProfiles.agent.profile')"
          :description="t('agentProfiles.model.profileHint')"
          label-for="agent-profile-select"
        >
          <SettingsSelect
            id="agent-profile-select"
            :model-value="model.profileId"
            class="w-56"
            :options="profileOptions"
            data-testid="agent-profile-select"
            @update:model-value="onProfile"
          />
        </SettingsRow>
        <SettingsRow
          icon="lucide:users"
          :title="t('agentProfiles.agent.open', { name: profileName })"
          :description="t('agentProfiles.agent.openHint')"
          :to="`/ai/profiles/${model.profileId}`"
          data-testid="agent-profile-link"
        />
      </SettingsGroup>

      <AgentsRightsEditor
        :model-value="draft"
        :reset-key="model.subject"
        disabled
      />
      <p class="px-1 text-xs text-muted-foreground">
        {{ t('agentProfiles.agent.rightsHint', { name: profileName }) }}
      </p>
    </template>
  </section>
</template>
