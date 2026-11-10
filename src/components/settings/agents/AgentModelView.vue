<script setup lang="ts">
/**
 * One model of the local agent (spec 083 US5, FR-008): the in-app chat agent has no registration,
 * its subject is the model that answers. Shows the agent profiles the model is a member of (a switch
 * each) and a note for a model that is a member of profiles but not offered any more. The rights
 * are on the profiles' pages only: they depend on which profile is active. A switch applies on selection
 * without a save button (023 FR-021) and holds on every device, because the memberships are part
 * of the vault.
 */
import { toast } from 'vue-sonner'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { onVaultTablesChanged } from '~/composables/useVaultData'
import { kindLabelKey } from '~/lib/agents/profiles'

const { t } = useI18n()
const { errString } = useErrorString()
const router = useTabRouter()
const store = useAgentProfilesStore()

const modelId = computed(() => String(router.route.params.modelId ?? ''))
const subjectKey = computed(() => `model:${modelId.value}`)
const model = computed(() =>
  store.subjects.find((subject) => subject.subject === subjectKey.value),
)
const loaded = ref(false)

async function reloadAsync() {
  try {
    await store.reloadAsync()
  } catch (error) {
    toast.error(errString(error))
  } finally {
    loaded.value = true
  }
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
    </template>
  </section>
</template>
