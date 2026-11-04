<script setup lang="ts">
/**
 * The rights group "Passwörter" of an agent (spec 021 US5, FR-019): rows for a `SettingsGroup`
 * of the rights list. The row has the same levels as every other right (nothing, reading,
 * reading and changing); nothing is the default. Once it is on, one dropdown names the entries it
 * reaches: all of them (the default) or those with one tag of the vault. "Lesen und ändern" lets
 * the agent browser save new logins (spec 065 FR-036); within a tag they get that tag. Every
 * secret still asks on each call. Rights stored with several tags, from before the dropdown, stay
 * as they are and show as one more choice until another is picked.
 */
import type { Level, PasswordRights } from '~/lib/agents/rights'

const props = defineProps<{ modelValue: PasswordRights | null }>()
const emit = defineEmits<{ 'update:modelValue': [PasswordRights | null] }>()

const { t } = useI18n()

const levelOptions = useRightLevelOptions()
const passwords = usePasswordsStore()

onMounted(() => {
  if (!passwords.hasLoadedOnce) void passwords.reloadAsync()
})

/** The choice "Alle"; no tag has an empty name. */
const ALL = ''
/** The choice that stands for the several tags of rights stored earlier. */
const SEVERAL = '\u0000several'

const level = computed<Level>(() =>
  !props.modelValue
    ? 'none'
    : props.modelValue.level === 'readWrite'
      ? 'write'
      : 'read',
)

const tags = computed(() => props.modelValue?.tags ?? null)
const several = computed(() => (tags.value?.length ?? 0) > 1)

const scope = computed(() => {
  if (!tags.value) return ALL
  return several.value ? SEVERAL : (tags.value[0] ?? ALL)
})

const scopeOptions = computed(() => {
  const names = new Set(passwords.displayTags.map((tag) => tag.name))
  for (const tag of tags.value ?? []) names.add(tag)
  return [
    { value: ALL, label: t('mcpAgents.passwords.all') },
    ...(several.value
      ? [{ value: SEVERAL, label: (tags.value ?? []).join(', ') }]
      : []),
    ...[...names]
      .sort((a, b) => a.localeCompare(b))
      .map((name) => ({ value: name, label: name })),
  ]
})

/** The rights with the tag for new entries: within tags one of them, else the stored one. */
function withAccess(
  access: 'read' | 'readWrite',
  scopeTags: string[] | null,
  defaultTag: string | null | undefined,
): PasswordRights {
  if (access === 'read') return { level: 'read', tags: scopeTags }
  const kept =
    defaultTag && (!scopeTags || scopeTags.includes(defaultTag))
      ? defaultTag
      : (scopeTags?.[0] ?? null)
  return { level: 'readWrite', tags: scopeTags, defaultTag: kept }
}

function setLevel(value: string) {
  if (value === 'none') {
    emit('update:modelValue', null)
    return
  }
  const access = value === 'write' ? 'readWrite' : 'read'
  const current = props.modelValue
  emit(
    'update:modelValue',
    current
      ? withAccess(access, current.tags, current.defaultTag)
      : { level: access, tags: null },
  )
}

function setScope(value: string) {
  const current = props.modelValue
  if (!current || value === SEVERAL) return
  const scopeTags = value === ALL ? null : [value]
  emit(
    'update:modelValue',
    withAccess(current.level, scopeTags, scopeTags ? value : null),
  )
}
</script>

<template>
  <SettingsRow
    :title="t('mcpAgents.passwords.title')"
    :description="t('mcpAgents.passwords.enabledHint')"
    label-for="mcp-passwords-level"
  >
    <SettingsSelect
      id="mcp-passwords-level"
      :model-value="level"
      class="w-44"
      :options="levelOptions"
      data-testid="mcp-passwords-level"
      @update:model-value="setLevel"
    />
  </SettingsRow>
  <li
    v-if="modelValue"
    class="flex flex-col gap-1 border-l-4 border-primary/60 bg-background/60 py-3 pr-4 pl-4"
    data-testid="mcp-passwords-scope"
  >
    <label class="text-sm font-medium" for="mcp-passwords-tag">
      {{ t('mcpAgents.passwords.scope') }}
    </label>
    <SettingsSelect
      id="mcp-passwords-tag"
      :model-value="scope"
      class="w-full"
      :options="scopeOptions"
      data-testid="mcp-passwords-tag"
      @update:model-value="setScope"
    />
    <p
      v-if="modelValue.level === 'readWrite' && tags"
      class="text-xs text-muted-foreground"
    >
      {{ t('mcpAgents.passwords.newEntriesTag') }}
    </p>
    <p class="pt-1 text-xs text-muted-foreground">
      {{ t('mcpAgents.passwords.secretHint') }}
    </p>
  </li>
</template>
