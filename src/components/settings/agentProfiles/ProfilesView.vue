<script setup lang="ts">
/**
 * "KI → Agentenprofile" (spec 083, US2, US3): the agent profiles of the vault, "Standard" first,
 * each with how many agents are in it (agents of every device and models), and a field that adds a
 * profile. A profile holds the rights for every kind of access at once and applies to every agent
 * in it. Two profiles may have the same name, so each row shows its count. Adding a profile runs a
 * guardrail action (`settings.agents.profiles.create`), which no agent can call; the new profile
 * gives nothing until its rights are set, so its page opens next.
 */
import { toast } from 'vue-sonner'
import { profileLabel } from '~/lib/agents/profiles'
import type { AgentProfileView } from '~/types/bindings/AgentProfileView'

const { t } = useI18n()
const { errString } = useErrorString()
const fieldLabels = useFieldLabels()
const router = useTabRouter()
const store = useAgentProfilesStore()
const create = useActionOrThrow('settings.agents.profiles.create')

const name = ref('')
const busy = ref(false)

async function reloadAsync() {
  try {
    await store.reloadAsync()
  } catch (error) {
    toast.error(errString(error))
  }
}

function describe(profile: AgentProfileView): string {
  const count = t(
    'agentProfiles.count',
    { count: profile.memberCount },
    profile.memberCount,
  )
  return profile.isStandard
    ? `${count} · ${t('agentProfiles.standardHint')}`
    : count
}

async function createAsync() {
  if (busy.value) return
  busy.value = true
  try {
    const created = (await create({ name: name.value })) as AgentProfileView
    name.value = ''
    router.push(`/ai/profiles/${created.id}`)
  } catch (error) {
    toast.error(errString(error))
  } finally {
    busy.value = false
  }
}

onMounted(reloadAsync)
</script>

<template>
  <section class="flex flex-col gap-3" data-testid="agent-profiles">
    <p class="px-1 text-sm text-muted-foreground">
      {{ t('agentProfiles.intro') }}
    </p>

    <SettingsGroup>
      <SettingsRow
        v-for="profile in store.profiles"
        :key="profile.id"
        icon="lucide:users"
        :title="profileLabel(t, profile.id, profile.name)"
        :description="describe(profile)"
        :to="`/ai/profiles/${profile.id}`"
        data-testid="agent-profile-row"
        :data-profile-id="profile.id"
        :data-standard="profile.isStandard || undefined"
      />
    </SettingsGroup>

    <SettingsGroup>
      <SettingsRow
        :title="t('agentProfiles.create.label')"
        :description="t('agentProfiles.create.hint')"
        label-for="agent-profile-new"
      >
        <div class="w-56 max-w-full">
          <UiInput
            id="agent-profile-new"
            v-model="name"
            type="text"
            :placeholder="t('agentProfiles.create.placeholder')"
            :labels="fieldLabels.input.value"
            :disabled="busy"
            data-testid="agent-profile-new"
            @keydown.enter.prevent="createAsync"
          />
        </div>
      </SettingsRow>
    </SettingsGroup>
  </section>
</template>
