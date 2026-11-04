<script setup lang="ts">
/**
 * The rights group "Passwörter" of an agent (spec 021 US5, FR-019): rows for a `SettingsGroup`
 * of the rights list. The row has the same levels as every other right (nothing, reading, reading
 * and changing); nothing is the default. Once it is on, the entries it reaches follow inside the
 * same list, all of them or those with chosen tags, with the fields of a browser extension's
 * grant (spec 049). "Lesen und ändern" lets the agent browser save new logins (spec 065
 * FR-036) with the tag chosen for new entries, which within chosen tags is one of them. Every
 * secret still asks on each call. Empty tags stay editable; `rightsOf` keeps access off until a
 * tag is chosen, and registration stays disabled while the draft is incomplete.
 */
import { draftOf, type GrantDraft } from '~/lib/browserBridgeGrant'
import type { Level, PasswordRights } from '~/lib/agents/rights'

const props = defineProps<{ modelValue: PasswordRights | null }>()
const emit = defineEmits<{ 'update:modelValue': [PasswordRights | null] }>()

const { t } = useI18n()

const levelOptions = useRightLevelOptions()

/** What the editor may offer: reading and writing, every entry. */
const OFFERED = { access: 'readWrite' as const, scopeTags: null }

const draft = ref<GrantDraft>(draftOf('read', null, null))
watch(
  () => props.modelValue,
  (rights) => {
    if (rights)
      draft.value = draftOf(
        rights.level,
        rights.tags ?? null,
        rights.defaultTag ?? null,
      )
  },
  { immediate: true },
)

const level = computed<Level>(() =>
  !props.modelValue
    ? 'none'
    : props.modelValue.level === 'readWrite'
      ? 'write'
      : 'read',
)

function onDraft(next: GrantDraft) {
  draft.value = next
  emit('update:modelValue', {
    level: next.access,
    tags: next.scope === 'tags' ? [...next.tags] : null,
    ...(next.access === 'readWrite'
      ? { defaultTag: next.defaultTag || null }
      : {}),
  })
}

function setLevel(value: string) {
  if (value === 'none') {
    emit('update:modelValue', null)
    return
  }
  const access = value === 'write' ? 'readWrite' : 'read'
  if (!props.modelValue) {
    emit('update:modelValue', { level: access, tags: null })
    return
  }
  // Writing within chosen tags needs one of them for new entries.
  const { tags, defaultTag } = draft.value
  onDraft({
    ...draft.value,
    access,
    defaultTag:
      access === 'readWrite' &&
      draft.value.scope === 'tags' &&
      !tags.includes(defaultTag)
        ? (tags[0] ?? '')
        : defaultTag,
  })
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
    class="flex flex-col gap-3 border-l-4 border-primary/60 bg-background/60 py-3 pr-4 pl-4"
    data-testid="mcp-passwords-scope"
  >
    <BrowserBridgeGrantFields
      :model-value="draft"
      :declared="OFFERED"
      :declares-write="true"
      hide-access
      test-prefix="mcp-passwords"
      @update:model-value="onDraft"
    />
    <p class="px-1 text-xs text-muted-foreground">
      {{ t('mcpAgents.passwords.secretHint') }}
    </p>
  </li>
</template>
