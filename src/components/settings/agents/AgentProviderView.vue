<script setup lang="ts">
/**
 * One provider of models (spec 083 PR M): the agent profiles the whole provider is a member of, so
 * every model it has, a later one too, has their rights. Its models list below, each with its own
 * page for memberships of its own. Use it to give the LiteLLM of an employer other rights than a
 * private account. A switch applies on selection without a save button (023 FR-021) and holds on
 * every device, because the memberships are part of the vault.
 */
import { toast } from 'vue-sonner'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { onVaultTablesChanged } from '~/composables/useVaultData'
import { kindLabelKey, modelName } from '~/lib/agents/profiles'

const { t } = useI18n()
const { errString } = useErrorString()
const router = useTabRouter()
const store = useAgentProfilesStore()

const providerId = computed(() => String(router.route.params.providerId ?? ''))
const subjectKey = computed(() => `provider:${providerId.value}`)
const provider = computed(() =>
  store.subjects.find((subject) => subject.subject === subjectKey.value),
)
const models = computed(() =>
  store.subjects.filter(
    (subject) => subject.providerSubject === subjectKey.value,
  ),
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

function modelPath(subject: string): string {
  return `/ai/agents/model/${encodeURIComponent(subject.slice('model:'.length))}`
}

let disposed = false
let unlisten: UnlistenFn | null = null
onMounted(() => {
  void reloadAsync()
  void listen('mcp-agents-changed', () => reloadAsync()).then((stop) => {
    if (disposed) stop()
    else unlisten = stop
  })
})
onBeforeUnmount(() => {
  disposed = true
  unlisten?.()
})
watch(providerId, reloadAsync)
onVaultTablesChanged(['preferences'], reloadAsync)
</script>

<template>
  <section class="flex flex-col gap-3" data-testid="agent-provider-detail">
    <p
      v-if="loaded && (!provider || provider.missing)"
      class="px-1 text-sm text-muted-foreground"
      role="status"
    >
      {{ t('agentProfiles.provider.gone') }}
    </p>
    <template v-if="provider">
      <SettingsGroup>
        <SettingsRow
          :title="provider.name"
          :description="t(kindLabelKey(provider.kind))"
        />
      </SettingsGroup>

      <SettingsAgentProfilesMemberships
        :subject="provider.subject"
        :description="t('agentProfiles.provider.hint')"
      />

      <SettingsGroup :label="t('agentProfiles.models.title')">
        <SettingsRow
          v-for="model in models"
          :key="model.subject"
          icon="lucide:cpu"
          :to="modelPath(model.subject)"
          :title="modelName(model)"
        />
      </SettingsGroup>
    </template>
  </section>
</template>
