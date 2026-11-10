<script setup lang="ts">
/**
 * The agents that are members of one agent profile (spec 083 US3, FR-016): each with its kind
 * ("Externer Agent", "Abo-Agent", "Lokaler Agent") and the device it is registered on, and a field
 * that adds one from every agent and model of the vault that is not in it yet. An agent can be a
 * member of several profiles, so adding never takes it out of another, and removing it from this
 * one leaves the rest. "Standard" holds every agent, which stay in it, and every model unless the
 * user took it out, so a model has a bin there and can be added back (FR-006b). Both run the
 * guardrail action `settings.agents.profiles.assign`, which no agent can call.
 */
import { toast } from 'vue-sonner'
import { groupByProvider, kindLabelKey, modelName } from '~/lib/agents/profiles'
import type { AgentSubjectView } from '~/types/bindings/AgentSubjectView'

const props = defineProps<{
  profileId: string
  members: AgentSubjectView[]
}>()

const { t } = useI18n()
const { errString } = useErrorString()
const store = useAgentProfilesStore()

/** The profile "Standard" has the id the store's first entry carries. */
const standardId = computed(
  () => store.profiles.find((profile) => profile.isStandard)?.id ?? '',
)
const isStandard = computed(
  () => !!standardId.value && props.profileId === standardId.value,
)
const grouped = computed(() => groupByProvider(props.members))
const adding = ref('')
const busy = ref(false)

function where(subject: AgentSubjectView): string {
  const parts = [t(kindLabelKey(subject.kind))]
  if (subject.device) parts.push(subject.device)
  if (subject.missing) parts.push(t('agentProfiles.model.missing'))
  if (
    subject.kind === 'builtin' &&
    !subject.profiles.some((profile) => profile.id === props.profileId) &&
    subject.inherited.some((profile) => profile.id === props.profileId)
  ) {
    parts.push(t('agentProfiles.provider.viaProvider'))
  }
  return parts.join(' · ')
}

/** A model whose provider left "Standard" cannot return before the provider does. */
function providerLeftStandard(subject: AgentSubjectView): boolean {
  return (
    store.subjects.find((other) => other.subject === subject.providerSubject)
      ?.inStandard === false
  )
}

/** What the field adds: in "Standard" the models that left it, else everything not in the profile. */
const candidates = computed(() => [
  { value: '', label: t('agentProfiles.members.add') },
  ...store.subjects
    .filter((subject) =>
      isStandard.value
        ? !subject.inStandard && !providerLeftStandard(subject)
        : !subject.profiles
            .concat(subject.inherited)
            .some((profile) => profile.id === props.profileId),
    )
    .map((subject) => ({
      value: subject.subject,
      label:
        subject.kind === 'provider'
          ? t('agentProfiles.provider.candidate', { name: subject.name })
          : subject.name,
    })),
])
/**
 * The agents of "Standard" cannot be taken out of it; its models and providers can. In another
 * profile a model that is in only through its provider has no bin of its own: the provider's does it.
 */
const removable = (member: AgentSubjectView) => {
  if (!standardId.value) return false
  if (isStandard.value) return member.kind !== 'external'
  return member.profiles.some((profile) => profile.id === props.profileId)
}

async function setAsync(subject: string, member: boolean) {
  if (busy.value) return
  busy.value = true
  try {
    await store.setMemberAsync(subject, props.profileId, member)
  } catch (error) {
    toast.error(errString(error))
  } finally {
    busy.value = false
    adding.value = ''
  }
}

function onAdd(subject: string) {
  if (!subject) return
  void setAsync(subject, true)
}
</script>

<template>
  <SettingsGroup
    :label="t('agentProfiles.members.title')"
    data-testid="agent-profile-members"
  >
    <template v-for="member in grouped.rest" :key="member.subject">
      <SettingsRow
        icon="lucide:bot"
        :title="member.name"
        :description="where(member)"
        data-testid="agent-profile-member"
        :data-subject="member.subject"
      >
        <DeleteButton
          v-if="removable(member)"
          :label="t('agentProfiles.members.removeNamed', { name: member.name })"
          :disabled="busy"
          data-testid="agent-profile-member-remove"
          @click="setAsync(member.subject, false)"
        />
      </SettingsRow>
    </template>
    <AgentsProviderGroup
      v-for="group in grouped.groups"
      :key="group.key"
      :label="group.provider ?? t('agentProfiles.models.local')"
      :count="group.items.length"
    >
      <SettingsRow
        v-if="group.providerSubject"
        icon="lucide:layers"
        :title="t('agentProfiles.provider.memberOfProvider')"
        :description="t('agentProfiles.kind.provider')"
        data-testid="agent-profile-member"
        :data-subject="group.providerSubject.subject"
      >
        <DeleteButton
          v-if="removable(group.providerSubject)"
          :label="
            t('agentProfiles.members.removeNamed', {
              name: group.providerSubject.name,
            })
          "
          :disabled="busy"
          data-testid="agent-profile-member-remove"
          @click="setAsync(group.providerSubject.subject, false)"
        />
      </SettingsRow>
      <template v-for="member in group.items" :key="member.subject">
        <SettingsRow
          icon="lucide:bot"
          :title="modelName(member)"
          :description="where(member)"
          data-testid="agent-profile-member"
          :data-subject="member.subject"
        >
          <DeleteButton
            v-if="removable(member)"
            :label="
              t('agentProfiles.members.removeNamed', { name: member.name })
            "
            :disabled="busy"
            data-testid="agent-profile-member-remove"
            @click="setAsync(member.subject, false)"
          />
        </SettingsRow>
      </template>
    </AgentsProviderGroup>
    <SettingsRow
      v-if="members.length === 0"
      :title="t('agentProfiles.members.none')"
    />
    <SettingsRow
      v-if="standardId && (!isStandard || candidates.length > 1)"
      :title="t('agentProfiles.members.addLabel')"
      label-for="agent-profile-add"
    >
      <SettingsSelect
        id="agent-profile-add"
        :model-value="adding"
        class="w-72 max-w-full"
        :options="candidates"
        :disabled="busy"
        data-testid="agent-profile-add"
        @update:model-value="onAdd"
      />
    </SettingsRow>
  </SettingsGroup>
</template>
