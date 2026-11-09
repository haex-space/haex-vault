<script setup lang="ts">
/**
 * One agent profile (spec 083, US2, US3, FR-005, FR-009, FR-016): its name, its rights for every
 * kind of access at once, the agents in it, and deleting it. The rights apply on selection without a
 * save button (023 FR-021) and reach every connected agent in the profile from its next call, with
 * no question about giving more (FR-016). Every change runs a guardrail action
 * (`settings.agents.profiles.*`), which no agent can call.
 */
import { toast } from 'vue-sonner'
import { invoke } from '@tauri-apps/api/core'
import { onVaultTablesChanged } from '~/composables/useVaultData'
import { profileLabel } from '~/lib/agents/profiles'
import {
  passwordsComplete,
  rightsDraftOf,
  rightsOf,
  type Rights,
  type RightsDraft,
} from '~/lib/agents/rights'
import type { AgentProfileDetail } from '~/types/bindings/AgentProfileDetail'

const { t } = useI18n()
const { errString } = useErrorString()
const fieldLabels = useFieldLabels()
const router = useTabRouter()
const mcp = useMcpAgentsStore()
const store = useAgentProfilesStore()
const setRights = useActionOrThrow('settings.agents.profiles.rights.set')
const rename = useActionOrThrow('settings.agents.profiles.rename')
const remove = useActionOrThrow('settings.agents.profiles.delete')
const activation = useAgentProfileActivation()

const profileId = computed(() => String(router.route.params.profileId ?? ''))
const profile = ref<AgentProfileDetail | null>(null)
const draft = ref<RightsDraft>(rightsDraftOf(null))
const name = ref('')
const loaded = ref(false)
const confirmDelete = ref(false)
/**
 * Set from deleting on: the deletion's change event reloads a profile that is gone by then, and
 * that failure is no error worth a toast.
 */
let leaving = false
let rightsSaveQueue = Promise.resolve()
let pendingRights = 0

const label = computed(() =>
  profile.value ? profileLabel(t, profile.value.id, profile.value.name) : '',
)
const memberCount = computed(() => profile.value?.members.length ?? 0)

async function reloadAsync() {
  const sameProfile = profile.value?.id === profileId.value
  try {
    const next = await invoke<AgentProfileDetail>('agent_profile_get', {
      args: { id: profileId.value },
    })
    profile.value = next
    name.value = next.name
    // While a save is queued the draft is what the user just chose; the reload that follows the
    // last save puts the stored rights in. Moving to another profile always resets the draft.
    if (sameProfile && pendingRights > 0) return
    const stored = rightsDraftOf(next.rights)
    // Keep an empty tag draft editable after its save revoked access. A failed save restores the
    // actual rights.
    const keepPasswords =
      sameProfile &&
      !passwordsComplete(draft.value.passwords) &&
      !stored.passwords
    draft.value = keepPasswords
      ? { ...stored, passwords: draft.value.passwords }
      : stored
  } catch (error) {
    profile.value = null
    if (!leaving) toast.error(errString(error))
  } finally {
    loaded.value = true
  }
}

async function runAsync(action: () => Promise<unknown>) {
  try {
    await action()
  } catch (error) {
    toast.error(errString(error))
  }
  await reloadAsync()
}

function saveRights(rights: Rights) {
  const idAtChange = profileId.value
  pendingRights += 1
  rightsSaveQueue = rightsSaveQueue.then(async () => {
    await runAsync(() => setRights({ id: idAtChange, rights }))
    pendingRights -= 1
    if (pendingRights === 0) await reloadAsync()
  })
}

/** Any rights group changed: saved with the others at once. */
function onDraft(next: RightsDraft) {
  draft.value = next
  saveRights(rightsOf(next))
}

async function renameAsync() {
  const current = profile.value
  if (!current || current.isStandard) return
  if (name.value.trim() === current.name) return
  await runAsync(() => rename({ id: current.id, name: name.value }))
}

async function deleteProfileAsync() {
  leaving = true
  try {
    await remove({ id: profileId.value })
  } catch (error) {
    leaving = false
    toast.error(errString(error))
    return
  }
  router.push('/ai/profiles')
}

onMounted(async () => {
  await Promise.all([reloadAsync(), store.reloadAsync().catch(() => {})])
})
onBeforeUnmount(() => {
  leaving = true
})
watch(profileId, reloadAsync)
onVaultTablesChanged(['preferences'], reloadAsync)
watch(() => mcp.changes, reloadAsync)
</script>

<template>
  <section class="flex flex-col gap-3" data-testid="agent-profile">
    <p
      v-if="loaded && !profile"
      class="px-1 text-sm text-muted-foreground"
      role="status"
    >
      {{ t('agentProfiles.profile.gone') }}
    </p>
    <template v-if="profile">
      <SettingsGroup>
        <SettingsRow
          v-if="profile.isStandard"
          :title="label"
          :description="t('agentProfiles.standardHint')"
          data-testid="agent-profile-name"
        />
        <SettingsRow
          v-else
          :title="t('agentProfiles.profile.name')"
          label-for="agent-profile-name-input"
        >
          <div class="w-56 max-w-full">
            <UiInput
              id="agent-profile-name-input"
              v-model="name"
              type="text"
              :labels="fieldLabels.input.value"
              data-testid="agent-profile-name-input"
              @blur="renameAsync"
              @keydown.enter.prevent="renameAsync"
            />
          </div>
        </SettingsRow>
        <SettingsRow
          :title="t('agentProfiles.profile.active')"
          :description="
            profile.isActive
              ? t('agentProfiles.profile.isActive')
              : t('agentProfiles.activeHint')
          "
          data-testid="agent-profile-active-row"
          :data-active="profile.isActive || undefined"
        >
          <UiButton
            v-if="!profile.isActive"
            type="button"
            variant="outline"
            data-testid="agent-profile-activate"
            @click="activation.activateAsync(profile.id)"
          >
            {{ t('agentProfiles.profile.activate') }}
          </UiButton>
        </SettingsRow>
        <SettingsRow
          :title="t('agentProfiles.profile.members')"
          :description="
            t('agentProfiles.count', { count: memberCount }, memberCount)
          "
          data-testid="agent-profile-count"
        />
      </SettingsGroup>

      <AgentsRightsEditor
        :model-value="draft"
        :reset-key="profile.id"
        @update:model-value="onDraft"
      />

      <SettingsAgentProfilesProfileMembers
        :profile-id="profile.id"
        :members="profile.members"
      />

      <SettingsGroup v-if="!profile.isStandard">
        <div class="flex flex-wrap items-center justify-end gap-2 p-4">
          <template v-if="confirmDelete">
            <span class="text-sm" data-testid="agent-profile-delete-text">{{
              t(
                'agentProfiles.profile.deleteConfirm',
                { count: memberCount },
                memberCount,
              )
            }}</span>
            <UiButton
              type="button"
              variant="outline"
              @click="confirmDelete = false"
            >
              {{ t('agentProfiles.profile.cancel') }}
            </UiButton>
            <UiButton
              type="button"
              variant="destructive"
              data-testid="agent-profile-delete-confirm"
              @click="deleteProfileAsync"
            >
              {{ t('agentProfiles.profile.delete') }}
            </UiButton>
          </template>
          <UiButton
            v-else
            type="button"
            variant="outline"
            data-testid="agent-profile-delete"
            @click="confirmDelete = true"
          >
            {{ t('agentProfiles.profile.delete') }}
          </UiButton>
        </div>
      </SettingsGroup>
    </template>
  </section>
</template>
