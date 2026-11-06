<script setup lang="ts">
/**
 * The rights group "Passwörter" of an agent (spec 021 US5, FR-019): rows for a `SettingsGroup`
 * of the rights list. The row has the same levels as every other right (nothing, reading,
 * reading and changing); nothing is the default. Once it is on, one dropdown names the entries it
 * reaches: all of them (the default) or those with the chosen tags of the vault. "Lesen und
 * ändern" lets the agent browser save new logins (spec 065 FR-036); within tags they get one of
 * them, which the user picks when there are several. Every secret still asks on each call.
 */
import type { Level, PasswordRights } from '~/lib/agents/rights'

const props = defineProps<{
  modelValue: PasswordRights | null
  disabled?: boolean
}>()
const emit = defineEmits<{ 'update:modelValue': [PasswordRights | null] }>()

const { t } = useI18n()

const levelOptions = useRightLevelOptions()
const passwords = usePasswordsStore()

onMounted(() => {
  if (!passwords.hasLoadedOnce) void passwords.reloadAsync()
})

const level = computed<Level>(() =>
  !props.modelValue
    ? 'none'
    : props.modelValue.level === 'readWrite'
      ? 'write'
      : 'read',
)

const tags = computed(() => props.modelValue?.tags ?? [])

const tagChoices = computed(() => {
  const names = new Set(passwords.displayTags.map((tag) => tag.name))
  for (const tag of tags.value) names.add(tag)
  return [...names].sort((a, b) => a.localeCompare(b))
})

/** The rights with the tag for new entries: within tags one of them, with all entries none (a
 * tag stored by an earlier editor would apply without showing anywhere). */
function withAccess(
  access: 'read' | 'readWrite',
  scopeTags: string[] | null,
  defaultTag: string | null | undefined,
): PasswordRights {
  if (access === 'read') return { level: 'read', tags: scopeTags }
  if (!scopeTags) return { level: 'readWrite', tags: null, defaultTag: null }
  const kept =
    defaultTag && scopeTags.includes(defaultTag)
      ? defaultTag
      : (scopeTags[0] ?? null)
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

/** The chosen tags; none is all entries. */
function setTags(chosen: string[]) {
  const current = props.modelValue
  if (!current) return
  // All entries: new entries get no tag any more.
  emit(
    'update:modelValue',
    chosen.length > 0
      ? withAccess(current.level, chosen, current.defaultTag)
      : withAccess(current.level, null, null),
  )
}

function setDefaultTag(tag: string) {
  const current = props.modelValue
  if (!current) return
  emit('update:modelValue', withAccess(current.level, current.tags, tag))
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
      :disabled="disabled"
      data-testid="mcp-passwords-level"
      @update:model-value="setLevel"
    />
  </SettingsRow>
  <AgentsRightsInset v-if="modelValue" data-testid="mcp-passwords-scope">
    <label class="text-sm font-medium" for="mcp-passwords-tag">
      {{ t('mcpAgents.passwords.scope') }}
    </label>
    <SettingsMultiSelect
      id="mcp-passwords-tag"
      :model-value="tags"
      class="w-full"
      :options="tagChoices"
      :all-label="t('mcpAgents.passwords.all')"
      :disabled="disabled"
      data-testid="mcp-passwords-tag"
      @update:model-value="setTags"
    />
    <template v-if="modelValue.level === 'readWrite' && tags.length === 1">
      <p class="text-xs text-muted-foreground">
        {{ t('mcpAgents.passwords.newEntriesTag') }}
      </p>
    </template>
    <template v-else-if="modelValue.level === 'readWrite' && tags.length > 1">
      <label class="pt-2 text-sm font-medium" for="mcp-passwords-default-tag">
        {{ t('mcpAgents.passwords.defaultTag') }}
      </label>
      <SettingsSelect
        id="mcp-passwords-default-tag"
        :model-value="modelValue.defaultTag ?? ''"
        class="w-full"
        :options="tags.map((tag) => ({ value: tag, label: tag }))"
        :disabled="disabled"
        data-testid="mcp-passwords-default-tag"
        @update:model-value="setDefaultTag"
      />
      <p class="text-xs text-muted-foreground">
        {{ t('mcpAgents.passwords.newEntriesTag') }}
      </p>
    </template>
    <p class="pt-1 text-xs text-muted-foreground">
      {{ t('mcpAgents.passwords.secretHint') }}
    </p>
  </AgentsRightsInset>
</template>
