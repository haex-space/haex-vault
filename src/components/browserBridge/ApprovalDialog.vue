<script setup lang="ts">
/**
 * May a browser extension connect, and with which grant? (spec 049, US1, FR-008 to FR-010,
 * contracts/tauri-commands.md). Shows what the client claims (its name, marked as its own claim),
 * its fingerprint and origin, and what it declared; the user picks the kind and the scope, never
 * wider than declared, and a default tag for new entries. Closing the dialog declines once, without
 * remembering. Rust checks the answer again; an answer it refuses counts as declining.
 */
import type { ApprovalQuestion } from '~/types/bindings/ApprovalQuestion'
import type { BridgeAccess } from '~/types/bindings/BridgeAccess'

const { t } = useI18n()
const { errString } = useErrorString()
const fieldLabels = useFieldLabels()
const store = useBrowserBridgeStore()
const passwords = usePasswordsStore()

const question = computed<ApprovalQuestion | null>(() =>
  store.shown?.question.kind === 'approval' ? store.shown.question : null,
)

const access = ref<BridgeAccess>('read')
const scope = ref<'all' | 'tags'>('all')
const chosenTags = ref<string[]>([])
const newTag = ref('')
const defaultTag = ref('')
const remember = ref(false)
const failure = ref<string | null>(null)

/** The tags the user may pick: the declared ones, or every tag of the vault. */
const tagChoices = computed(() => {
  const declared = question.value?.declared.scopeTags
  if (declared) return declared
  const names = passwords.displayTags.map((tag) => tag.name)
  return [...new Set([...names, ...chosenTags.value])].sort((a, b) =>
    a.localeCompare(b),
  )
})

const needsDefaultTag = computed(
  () => access.value === 'readWrite' && scope.value === 'tags',
)
const canAllow = computed(
  () =>
    (scope.value === 'all' || chosenTags.value.length > 0) &&
    (!needsDefaultTag.value || chosenTags.value.includes(defaultTag.value)),
)

watch(
  () => store.shown?.promptId,
  () => {
    const asked = question.value
    failure.value = null
    remember.value = false
    newTag.value = ''
    if (!asked) return
    access.value = asked.declared.access
    scope.value = asked.declared.scopeTags ? 'tags' : 'all'
    chosenTags.value = [...(asked.declared.scopeTags ?? [])]
    defaultTag.value = chosenTags.value[0] ?? ''
    if (!passwords.hasLoadedOnce) void passwords.reloadAsync()
  },
  { immediate: true },
)

function toggleTag(name: string, on: boolean) {
  chosenTags.value = on
    ? [...new Set([...chosenTags.value, name])]
    : chosenTags.value.filter((tag) => tag !== name)
  if (!chosenTags.value.includes(defaultTag.value))
    defaultTag.value = chosenTags.value[0] ?? ''
}

function addTag() {
  const name = newTag.value.trim()
  if (!name) return
  toggleTag(name, true)
  newTag.value = ''
}

/** The fingerprint in groups of four, as the settings show it. */
const fingerprint = computed(
  () => question.value?.client.clientId.match(/.{1,4}/g)?.join(' ') ?? '',
)

async function answerAsync(allow: boolean) {
  const shown = store.shown
  if (!shown) return
  const tags = scope.value === 'tags' ? chosenTags.value : null
  try {
    await store.answerAsync(shown.promptId, {
      kind: 'approval',
      allow,
      remember: remember.value,
      access: allow ? access.value : null,
      scopeTags: allow ? tags : null,
      defaultTag: allow && defaultTag.value ? defaultTag.value : null,
    })
  } catch (error) {
    failure.value = errString(error)
  }
}

function onUpdateOpen(open: boolean) {
  if (!open) void answerAsync(false)
}
</script>

<template>
  <UiDrawerModal
    v-if="question"
    :open="true"
    :title="t('browserBridge.approval.title')"
    @update:open="onUpdateOpen"
  >
    <template #content>
      <div class="space-y-4" data-testid="bridge-approval">
        <dl class="grid grid-cols-[auto_1fr] gap-x-3 gap-y-1 text-sm">
          <dt class="text-muted-foreground">
            {{ t('browserBridge.client.name') }}
          </dt>
          <dd class="min-w-0 break-words" data-testid="bridge-approval-name">
            {{ question.client.clientName || t('browserBridge.client.noName') }}
            <span class="text-xs text-muted-foreground">
              ({{ t('browserBridge.client.claimed') }})
            </span>
          </dd>
          <dt class="text-muted-foreground">
            {{ t('browserBridge.client.fingerprint') }}
          </dt>
          <dd
            class="min-w-0 font-mono text-xs break-all"
            data-testid="bridge-approval-fingerprint"
          >
            {{ fingerprint }}
          </dd>
          <dt class="text-muted-foreground">
            {{ t('browserBridge.client.origin') }}
          </dt>
          <dd
            class="min-w-0 font-mono text-xs break-all"
            data-testid="bridge-approval-origin"
          >
            {{ question.client.origin }}
          </dd>
        </dl>
        <p class="text-sm">
          {{
            t('browserBridge.approval.declared', {
              access: t(`browserBridge.access.${question.declared.access}`),
              scope: question.declared.scopeTags
                ? question.declared.scopeTags.join(', ')
                : t('browserBridge.scope.all'),
            })
          }}
        </p>

        <SettingsGroup
          v-model="access"
          radio
          :label="t('browserBridge.access.label')"
        >
          <SettingsOptionRow
            type="radio"
            value="read"
            :title="t('browserBridge.access.read')"
            data-testid="bridge-approval-read"
          />
          <SettingsOptionRow
            type="radio"
            value="readWrite"
            :title="t('browserBridge.access.readWrite')"
            :disabled="!question.declaresWrite"
            data-testid="bridge-approval-read-write"
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
            :disabled="question.declared.scopeTags !== null"
            data-testid="bridge-approval-scope-all"
          />
          <SettingsOptionRow
            type="radio"
            value="tags"
            :title="t('browserBridge.scope.tags')"
            data-testid="bridge-approval-scope-tags"
          />
        </SettingsGroup>

        <template v-if="scope === 'tags'">
          <SettingsGroup v-if="tagChoices.length > 0">
            <SettingsOptionRow
              v-for="tag in tagChoices"
              :key="tag"
              type="checkbox"
              :title="tag"
              :checked="chosenTags.includes(tag)"
              :data-testid="`bridge-approval-tag-${tag}`"
              @change="toggleTag(tag, $event)"
            />
          </SettingsGroup>
          <form
            v-if="question.declared.scopeTags === null"
            class="flex gap-2"
            @submit.prevent="addTag"
          >
            <UiInput
              v-model="newTag"
              :label="t('browserBridge.scope.newTag')"
              :labels="fieldLabels.input.value"
              data-testid="bridge-approval-new-tag"
            />
            <UiButton type="submit" size="sm" variant="outline">
              {{ t('browserBridge.scope.addTag') }}
            </UiButton>
          </form>
        </template>

        <label
          v-if="access === 'readWrite'"
          class="flex flex-col gap-1 text-sm"
          for="bridge-approval-default-tag"
        >
          {{ t('browserBridge.defaultTag.label') }}
          <SettingsSelect
            id="bridge-approval-default-tag"
            v-model="defaultTag"
            class="w-full"
            :options="[
              ...(needsDefaultTag
                ? []
                : [{ value: '', label: t('browserBridge.defaultTag.none') }]),
              ...(scope === 'tags' ? chosenTags : tagChoices).map((tag) => ({
                value: tag,
                label: tag,
              })),
            ]"
            data-testid="bridge-approval-default-tag"
          />
          <span class="text-xs text-muted-foreground">
            {{ t('browserBridge.defaultTag.hint') }}
          </span>
        </label>

        <label class="flex items-center gap-2 text-sm">
          <ShadcnCheckbox
            v-model="remember"
            data-testid="bridge-approval-remember"
          />
          {{ t('browserBridge.approval.remember') }}
        </label>
        <p class="text-xs text-muted-foreground">
          {{
            remember
              ? t('browserBridge.approval.rememberedHint')
              : t('browserBridge.approval.sessionHint')
          }}
        </p>
        <p v-if="failure" class="text-sm text-destructive" role="alert">
          {{ failure }}
        </p>
      </div>
    </template>
    <template #footer>
      <div class="flex justify-end gap-2">
        <UiButton
          type="button"
          size="sm"
          variant="outline"
          data-testid="bridge-approval-deny"
          @click="answerAsync(false)"
        >
          {{ t('browserBridge.approval.deny') }}
        </UiButton>
        <UiButton
          type="button"
          size="sm"
          :disabled="!canAllow"
          data-testid="bridge-approval-allow"
          @click="answerAsync(true)"
        >
          {{ t('browserBridge.approval.allow') }}
        </UiButton>
      </div>
    </template>
  </UiDrawerModal>
</template>
