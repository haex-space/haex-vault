<script setup lang="ts">
/**
 * Kind, scope and default tag of a browser extension's grant (spec 049, FR-010), never wider than
 * it declared: the approval question and the editor in the settings show the same fields. The tags
 * offered are the declared ones, or every tag of the vault when it declared all entries.
 */
import type { BridgeDeclaredGrant } from '~/types/bindings/BridgeDeclaredGrant'
import type { BridgeAccess } from '~/types/bindings/BridgeAccess'
import { needsDefaultTag, type GrantDraft } from '~/lib/browserBridgeGrant'

const props = defineProps<{
  declared: BridgeDeclaredGrant
  declaresWrite: boolean
  /** Prefix of the test ids, e.g. `bridge-approval`. */
  testPrefix: string
}>()

const draft = defineModel<GrantDraft>({ required: true })

const { t } = useI18n()
const fieldLabels = useFieldLabels()
const passwords = usePasswordsStore()
const newTag = ref('')

onMounted(() => {
  if (!passwords.hasLoadedOnce) void passwords.reloadAsync()
})

const access = computed({
  get: () => draft.value.access,
  set: (value: BridgeAccess) =>
    (draft.value = { ...draft.value, access: value }),
})
const scope = computed({
  get: () => draft.value.scope,
  set: (value: 'all' | 'tags') =>
    (draft.value = { ...draft.value, scope: value }),
})
const defaultTag = computed({
  get: () => draft.value.defaultTag,
  set: (value: string) => (draft.value = { ...draft.value, defaultTag: value }),
})
const mustHaveDefault = computed(() => needsDefaultTag(draft.value))

const tagChoices = computed(() => {
  const declared = props.declared.scopeTags
  if (declared) return declared
  const names = passwords.displayTags.map((tag) => tag.name)
  return [...new Set([...names, ...draft.value.tags])].sort((a, b) =>
    a.localeCompare(b),
  )
})

function toggleTag(name: string, on: boolean) {
  const tags = on
    ? [...new Set([...draft.value.tags, name])]
    : draft.value.tags.filter((tag) => tag !== name)
  const keep = tags.includes(draft.value.defaultTag)
  draft.value = {
    ...draft.value,
    tags,
    defaultTag: keep ? draft.value.defaultTag : (tags[0] ?? ''),
  }
}

function addTag() {
  const name = newTag.value.trim()
  if (!name) return
  toggleTag(name, true)
  newTag.value = ''
}
</script>

<template>
  <div class="space-y-4">
    <SettingsGroup
      v-model="access"
      radio
      :label="t('browserBridge.access.label')"
    >
      <SettingsOptionRow
        type="radio"
        value="read"
        :title="t('browserBridge.access.read')"
        :data-testid="`${testPrefix}-read`"
      />
      <SettingsOptionRow
        type="radio"
        value="readWrite"
        :title="t('browserBridge.access.readWrite')"
        :disabled="!declaresWrite"
        :data-testid="`${testPrefix}-read-write`"
      />
    </SettingsGroup>

    <SettingsGroup
      v-model="scope"
      radio
      :label="t('browserBridge.scope.label')"
    >
      <SettingsOptionRow
        type="radio"
        value="all"
        :title="t('browserBridge.scope.all')"
        :disabled="declared.scopeTags !== null"
        :data-testid="`${testPrefix}-scope-all`"
      />
      <SettingsOptionRow
        type="radio"
        value="tags"
        :title="t('browserBridge.scope.tags')"
        :data-testid="`${testPrefix}-scope-tags`"
      />
    </SettingsGroup>

    <template v-if="scope === 'tags'">
      <SettingsGroup v-if="tagChoices.length > 0">
        <SettingsOptionRow
          v-for="tag in tagChoices"
          :key="tag"
          type="checkbox"
          :title="tag"
          :checked="draft.tags.includes(tag)"
          :data-testid="`${testPrefix}-tag-${tag}`"
          @change="toggleTag(tag, $event)"
        />
      </SettingsGroup>
      <form
        v-if="declared.scopeTags === null"
        class="flex gap-2"
        @submit.prevent="addTag"
      >
        <UiInput
          v-model="newTag"
          :label="t('browserBridge.scope.newTag')"
          :labels="fieldLabels.input.value"
          :data-testid="`${testPrefix}-new-tag`"
        />
        <UiButton type="submit" size="sm" variant="outline">
          {{ t('browserBridge.scope.addTag') }}
        </UiButton>
      </form>
      <p
        v-if="draft.tags.length === 0"
        class="px-1 text-xs text-muted-foreground"
      >
        {{ t('browserBridge.scope.pickTag') }}
      </p>
    </template>

    <label
      v-if="access === 'readWrite'"
      class="flex flex-col gap-1 text-sm"
      :for="`${testPrefix}-default-tag`"
    >
      {{ t('browserBridge.defaultTag.label') }}
      <SettingsSelect
        :id="`${testPrefix}-default-tag`"
        v-model="defaultTag"
        class="w-full"
        :options="[
          ...(mustHaveDefault
            ? []
            : [{ value: '', label: t('browserBridge.defaultTag.none') }]),
          ...(scope === 'tags' ? draft.tags : tagChoices).map((tag) => ({
            value: tag,
            label: tag,
          })),
        ]"
        :data-testid="`${testPrefix}-default-tag`"
      />
      <span class="text-xs text-muted-foreground">
        {{ t('browserBridge.defaultTag.hint') }}
      </span>
    </label>
  </div>
</template>
