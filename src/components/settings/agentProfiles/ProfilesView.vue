<script setup lang="ts">
/**
 * "KI → Agentenprofile" (spec 083, US2, US3): the agent profiles of the vault, "Standard" first,
 * each with how many agents are in it (agents of every device and models), and a field that adds a
 * profile. A profile holds the rights for every kind of access at once and applies to every agent
 * in it. Two profiles may have the same name, so each row shows its count. Adding a profile (the
 * "+" next to the name field, or Enter) runs a guardrail action (`settings.agents.profiles.create`),
 * which no agent can call; its page opens next.
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
const activation = useAgentProfileActivation()

const name = ref('')
const busy = ref(false)

async function reloadAsync() {
  try {
    await store.reloadAsync()
  } catch (error) {
    toast.error(errString(error))
  }
}

const activeId = computed(
  () => store.profiles.find((profile) => profile.isActive)?.id ?? '',
)
const activeOptions = computed(() =>
  store.profiles.map((profile) => ({
    value: profile.id,
    label: profileLabel(t, profile.id, profile.name),
  })),
)

function describe(profile: AgentProfileView): string {
  const count = t(
    'agentProfiles.count',
    { count: profile.memberCount },
    profile.memberCount,
  )
  const parts = [count]
  if (profile.isActive) parts.push(t('agentProfiles.active'))
  if (profile.isStandard) parts.push(t('agentProfiles.standardHint'))
  return parts.join(' · ')
}

async function createAsync() {
  if (busy.value || !name.value.trim()) return
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
        :title="t('agentProfiles.activeLabel')"
        :description="t('agentProfiles.activeHint')"
        label-for="agent-profile-active"
      >
        <SettingsSelect
          id="agent-profile-active"
          :model-value="activeId"
          class="w-56 max-w-full"
          :options="activeOptions"
          data-testid="agent-profile-active"
          @update:model-value="activation.activateAsync"
        />
      </SettingsRow>
      <SettingsAgentProfilesToolRoundsSetting />
    </SettingsGroup>

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
        label-for="agent-profile-new"
      >
        <div class="flex w-72 max-w-full items-center gap-2">
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
          <UiButton
            type="button"
            variant="outline"
            size="icon"
            class="size-9 shrink-0"
            :aria-label="t('agentProfiles.create.add')"
            :tooltip="t('agentProfiles.create.add')"
            :loading="busy"
            :disabled="!name.trim()"
            data-testid="agent-profile-add"
            @click="createAsync"
          >
            <Icon name="lucide:plus" class="size-4" aria-hidden="true" />
          </UiButton>
        </div>
      </SettingsRow>
    </SettingsGroup>
  </section>
</template>
