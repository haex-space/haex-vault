<script setup lang="ts">
/**
 * One model of the local agent (spec 083 US5, FR-008): the in-app chat agent has no registration,
 * its subject is the model that answers. Shows the agent profiles the model is a member of (a switch
 * each), the rights of the one that applies (read only; changed on the profile's page), and a note
 * for a model that is a member of profiles but not offered any more. A switch applies on selection
 * without a save button (023 FR-021) and holds on every device, because the memberships are part
 * of the vault.
 */
import { toast } from 'vue-sonner'
import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { onVaultTablesChanged } from '~/composables/useVaultData'
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

let disposed = false
let unlistenProfiles: UnlistenFn | null = null
onMounted(() => {
  void reloadAsync()
  void listen('mcp-agents-changed', () => reloadAsync()).then((stop) => {
    if (disposed) stop()
    else unlistenProfiles = stop
  })
})
onBeforeUnmount(() => {
  disposed = true
  unlistenProfiles?.()
})
watch(modelId, reloadAsync)
watch(
  () => model.value?.profileId,
  () => void loadRightsAsync(),
)
onVaultTablesChanged(['preferences'], reloadAsync)
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

      <SettingsAgentProfilesMemberships
        :subject="model.subject"
        :description="t('agentProfiles.model.profileHint')"
      />

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
