<script setup lang="ts">
/**
 * The rights group "Kontakte" of an agent profile (spec 083 FR-032; spec 051 FR-030): what an agent
 * reaches in the address book, for all contacts and for single groups. Without an entry the agent is
 * asked at its first access, and "Immer erlauben" is written here. A refusal for all contacts takes
 * everything away; one for a group hides its members from a grant for all. Each choice emits the
 * whole group; the editor saves on selection.
 */
import { invoke } from '@tauri-apps/api/core'
import type { ContactGroup } from '@bindings/ContactGroup'
import type { AccessLevel, ContactsRights } from '~/lib/agents/rights'

const props = defineProps<{
  modelValue: ContactsRights
  disabled?: boolean
}>()
const emit = defineEmits<{ 'update:modelValue': [ContactsRights] }>()

const { t } = useI18n()

const groups = ref<ContactGroup[]>([])
onMounted(async () => {
  try {
    groups.value = await invoke<ContactGroup[]>('contacts_groups')
  } catch {
    // Without the names the rights of the groups still show, as groups of unknown name.
  }
})

/** The choice without an entry. */
const NONE = 'none'
type Choice = AccessLevel | typeof NONE

const levels: AccessLevel[] = ['read', 'readWrite', 'denied']

const allOptions = computed(() => [
  { value: NONE, label: t('mcpAgents.contacts.ask') },
  ...levels.map((level) => ({
    value: level,
    label: t(`mcpAgents.contacts.level.${level}`),
  })),
])
const groupOptions = computed(() => [
  { value: NONE, label: t('mcpAgents.contacts.likeAll') },
  ...levels.map((level) => ({
    value: level,
    label: t(`mcpAgents.contacts.level.${level}`),
  })),
])

/** The groups of the address book, then those with an entry whose group is gone. */
const rows = computed(() => {
  const entries = props.modelValue.groups ?? {}
  const known = groups.value.map((group) => ({
    id: group.id,
    name: group.name,
  }))
  const gone = Object.keys(entries)
    .filter((id) => !groups.value.some((group) => group.id === id))
    .map((id) => ({ id, name: t('mcpAgents.contacts.groupGone') }))
  return [...known, ...gone]
})

function chooseAll(next: string) {
  const { all: _removed, ...rest } = props.modelValue
  emit(
    'update:modelValue',
    next === NONE ? rest : { ...rest, all: next as AccessLevel },
  )
}

function chooseGroup(id: string, next: string) {
  const { [id]: _removed, ...others } = props.modelValue.groups ?? {}
  const groupsNext = next === NONE ? others : { ...others, [id]: next }
  emit('update:modelValue', {
    ...(props.modelValue.all ? { all: props.modelValue.all } : {}),
    ...(Object.keys(groupsNext).length > 0
      ? { groups: groupsNext as Record<string, AccessLevel> }
      : {}),
  })
}

const groupChoice = (id: string): Choice =>
  props.modelValue.groups?.[id] ?? NONE
</script>

<template>
  <SettingsGroup
    :label="t('mcpAgents.contacts.title')"
    data-testid="mcp-contacts-rights"
  >
    <SettingsRow
      :title="t('mcpAgents.contacts.all')"
      :description="t('mcpAgents.contacts.allHint')"
      icon="lucide:contact"
      label-for="mcp-contacts-all"
    >
      <SettingsSelect
        id="mcp-contacts-all"
        :model-value="modelValue.all ?? NONE"
        :options="allOptions"
        :disabled="disabled"
        data-testid="mcp-contacts-all"
        @update:model-value="chooseAll"
      />
    </SettingsRow>
    <SettingsRow
      v-for="group in rows"
      :key="group.id"
      :title="t('mcpAgents.contacts.group', { name: group.name })"
      icon="lucide:users"
      :label-for="`mcp-contacts-group-${group.id}`"
    >
      <SettingsSelect
        :id="`mcp-contacts-group-${group.id}`"
        :model-value="groupChoice(group.id)"
        :options="groupOptions"
        :disabled="disabled"
        :data-testid="`mcp-contacts-group-${group.id}`"
        @update:model-value="chooseGroup(group.id, $event)"
      />
    </SettingsRow>
  </SettingsGroup>
</template>
