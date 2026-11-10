<script setup lang="ts">
/**
 * May an agent use holzi, and in which agent profile? (spec 021, US1, FR-006, spec 083 US4, FR-013,
 * FR-014). Shows what the agent reports about itself, marked as its own claim, and lets the user choose an agent profile to make it a member of
 * ("Standard", no further profile, is preselected; one that registers again keeps its memberships).
 * The rights are shown read only, for every kind of access at once: those of the chosen profile
 * while it is the active one, else those of "Standard"; they change on the profile's page. It closes only through its own buttons (spec 065 FR-042): "Ablehnen" declines once,
 * without remembering. A profile that was deleted meanwhile is refused by
 * Rust; the dialog then reloads the profiles and stays open.
 */
import { toast } from 'vue-sonner'
import { invoke } from '@tauri-apps/api/core'
import {
  STANDARD_PROFILE_ID,
  profileErrorCode,
  profileLabel,
} from '~/lib/agents/profiles'
import { rightsDraftOf, type RightsDraft } from '~/lib/agents/rights'
import type { AgentProfileDetail } from '~/types/bindings/AgentProfileDetail'
import type { ProfileChoice } from '~/types/bindings/ProfileChoice'
import type { RegistrationQuestion } from '~/types/bindings/RegistrationQuestion'

const { t } = useI18n()
const { errString } = useErrorString()
const store = useMcpAgentsStore()
const profiles = useAgentProfilesStore()

const question = computed<RegistrationQuestion | null>(
  () => store.shown?.question ?? null,
)
const draft = ref<RightsDraft>(rightsDraftOf(null))
const choices = ref<ProfileChoice[]>([])
const profileId = ref(STANDARD_PROFILE_ID)
const remember = ref(false)

const options = computed(() =>
  choices.value.map((choice) => ({
    value: choice.id,
    label: profileLabel(t, choice.id, choice.name),
  })),
)

watch(
  () => store.shown?.requestId,
  () => {
    const asked = question.value
    remember.value = false
    if (!asked) return
    choices.value = asked.profiles
    profileId.value = asked.profileId
    draft.value = rightsDraftOf(asked.rights)
  },
  { immediate: true },
)

/** Reloads the profiles after Rust said one is gone, and selects "Standard" when the chosen one is. */
async function recoverAsync(error: unknown) {
  toast.error(errString(error))
  if (profileErrorCode(error) !== 'profile_missing') return
  try {
    await profiles.reloadAsync()
  } catch (reloadError) {
    toast.error(errString(reloadError))
    return
  }
  choices.value = profiles.profiles.map(({ id, name }) => ({ id, name }))
  if (!choices.value.some((choice) => choice.id === profileId.value)) {
    await selectAsync(STANDARD_PROFILE_ID)
  }
}

/** Shows the rights an agent that joins the chosen profile would have. */
async function selectAsync(id: string) {
  profileId.value = id
  try {
    let detail = await invoke<AgentProfileDetail>('agent_profile_get', {
      args: { id },
    })
    // A profile that is not the active one gives its members the rights of "Standard".
    if (!detail.isActive && !detail.isStandard) {
      detail = await invoke<AgentProfileDetail>('agent_profile_get', {
        args: { id: STANDARD_PROFILE_ID },
      })
    }
    if (profileId.value === id) draft.value = rightsDraftOf(detail.rights)
  } catch (error) {
    await recoverAsync(error)
  }
}

async function answerAsync(register: boolean) {
  const shown = store.shown
  const asked = question.value
  if (!shown || !asked) return
  try {
    await store.answerAsync(shown.requestId, {
      decision: register ? 'register' : 'deny',
      remember: register ? false : remember.value,
      profileId: profileId.value,
    })
  } catch (error) {
    await recoverAsync(error)
  }
}
</script>

<template>
  <UiDrawerModal
    v-if="question"
    :open="true"
    :title="t('mcpAgents.registration.title')"
    :dismissible="false"
  >
    <template #content>
      <div class="space-y-4" data-testid="mcp-registration">
        <dl class="grid grid-cols-[auto_1fr] gap-x-3 gap-y-1 text-sm">
          <dt class="text-muted-foreground">
            {{ t('mcpAgents.registration.name') }}
          </dt>
          <dd class="min-w-0 break-words" data-testid="mcp-registration-name">
            {{ question.clientName }}
            <span class="text-xs text-muted-foreground">
              ({{ t('mcpAgents.registration.claimed') }})
            </span>
          </dd>
          <template v-if="question.clientVersion">
            <dt class="text-muted-foreground">
              {{ t('mcpAgents.registration.version') }}
            </dt>
            <dd class="min-w-0 break-words">{{ question.clientVersion }}</dd>
          </template>
          <dt class="text-muted-foreground">
            {{ t('mcpAgents.registration.connection') }}
          </dt>
          <dd class="min-w-0">
            {{ t(`mcpAgents.transport.${question.transport}`) }}
          </dd>
        </dl>

        <SettingsGroup>
          <SettingsRow
            :title="t('agentProfiles.agent.profile')"
            :description="t('agentProfiles.registration.profileHint')"
            label-for="mcp-registration-profile"
          >
            <SettingsSelect
              id="mcp-registration-profile"
              :model-value="profileId"
              class="w-56"
              :options="options"
              data-testid="agent-profile-select"
              @update:model-value="selectAsync"
            />
          </SettingsRow>
        </SettingsGroup>
        <AgentsRightsEditor
          :model-value="draft"
          :reset-key="store.shown?.requestId"
          disabled
        />
        <p class="text-xs text-muted-foreground">
          {{ t('agentProfiles.registration.rightsHint') }}
        </p>

        <label class="flex items-center gap-2 text-sm">
          <ShadcnCheckbox
            v-model="remember"
            data-testid="mcp-registration-remember"
          />
          {{ t('mcpAgents.registration.remember') }}
        </label>
      </div>
    </template>
    <template #footer>
      <div class="flex justify-end gap-2">
        <UiButton
          type="button"
          size="sm"
          variant="outline"
          data-testid="mcp-registration-deny"
          @click="answerAsync(false)"
        >
          {{ t('mcpAgents.registration.deny') }}
        </UiButton>
        <UiButton
          type="button"
          size="sm"
          data-testid="mcp-registration-register"
          @click="answerAsync(true)"
        >
          {{ t('mcpAgents.registration.register') }}
        </UiButton>
      </div>
    </template>
  </UiDrawerModal>
</template>
