<script setup lang="ts">
/**
 * The rights group "Passwörter" of an agent (spec 021 US5, FR-019): off by default; when on, the
 * entries it reaches, all of them or those with chosen tags, with the fields of a browser
 * extension's grant (spec 049). Agents only read: no password action writes yet (research R13).
 * Every secret still asks on each call. Empty tags stay editable; `rightsOf` keeps access off
 * until a tag is chosen, and registration stays disabled while the draft is incomplete.
 */
import { draftOf, type GrantDraft } from '~/lib/browserBridgeGrant'
import type { PasswordRights } from '~/lib/agents/rights'

const props = defineProps<{ modelValue: PasswordRights | null }>()
const emit = defineEmits<{ 'update:modelValue': [PasswordRights | null] }>()

const { t } = useI18n()

const READ_ONLY = { access: 'read' as const, scopeTags: null }

const draft = ref<GrantDraft>(draftOf('read', null, null))
watch(
  () => props.modelValue,
  (rights) => {
    if (rights) draft.value = draftOf('read', rights.tags ?? null, null)
  },
  { immediate: true },
)

function toggle(on: boolean) {
  emit('update:modelValue', on ? { level: 'read', tags: null } : null)
}

function onDraft(next: GrantDraft) {
  draft.value = next
  emit('update:modelValue', {
    level: 'read',
    tags: next.scope === 'tags' ? [...next.tags] : null,
  })
}
</script>

<template>
  <SettingsGroup :label="t('mcpAgents.passwords.title')">
    <SettingsRow
      :title="t('mcpAgents.passwords.enabled')"
      :description="t('mcpAgents.passwords.enabledHint')"
      label-for="mcp-passwords-switch"
    >
      <ShadcnSwitch
        id="mcp-passwords-switch"
        :model-value="modelValue !== null"
        data-testid="mcp-passwords-switch"
        @update:model-value="toggle($event === true)"
      />
    </SettingsRow>
  </SettingsGroup>
  <BrowserBridgeGrantFields
    v-if="modelValue"
    :model-value="draft"
    :declared="READ_ONLY"
    :declares-write="false"
    test-prefix="mcp-passwords"
    @update:model-value="onDraft"
  />
  <p v-if="modelValue" class="px-1 text-xs text-muted-foreground">
    {{ t('mcpAgents.passwords.secretHint') }}
  </p>
</template>
