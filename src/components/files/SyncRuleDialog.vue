<script setup lang="ts">
/**
 * Creates or changes a sync rule (spec 061 US1, US2, US4 scenarios 1, 4 and 6, FR-001, FR-005,
 * FR-030a, FR-031): a name, two places from the folder picker, the direction (one way or both ways),
 * the delete mode and the interval. The dialog asks for consent when files would leave an encrypted
 * folder for a plain one, and points to encrypted folders when a place copied into is a storage
 * without one. The preview shows the plan of the rule as it stands in the dialog before it is saved.
 * A changed rule keeps its places until another folder is picked; the picker cannot open at a
 * stored place.
 */
import type { Sources } from '@bindings/Sources'
import type { SyncRuleDeleteMode } from '@bindings/SyncRuleDeleteMode'
import type { SyncRuleDirection } from '@bindings/SyncRuleDirection'
import type { SyncRule } from '@bindings/SyncRule'
import type { SyncRuleDraft } from '@bindings/SyncRuleDraft'
import { asSyncRuleError } from '~/composables/useSyncRules'
import {
  canSave,
  DEFAULT_INTERVAL_MINUTES,
  defaultDeleteMode,
  intervalSecs,
  lastName,
  needsDecryptConsent,
  showsCloudHint,
  targetSides,
} from '~/lib/files/syncRules'
import type { PickedFolder } from './FolderPicker.vue'

const props = defineProps<{
  sources: Sources | null
  /** The rule to change; `null` for a new one. */
  rule?: SyncRule | null
  /** The names of both places of the rule changed. */
  labels?: { a: string; b: string }
}>()

const open = defineModel<boolean>('open', { required: true })
const emit = defineEmits<{ saved: [] }>()

const { t } = useI18n()
const fieldLabels = useFieldLabels()
const { createAsync, updateAsync } = useSyncRules()
const previewing = ref(false)
const previewTarget = ref<{ id?: string; draft?: SyncRuleDraft } | null>(null)

const name = ref('')
const placeA = ref<PickedFolder | null>(null)
const placeB = ref<PickedFolder | null>(null)
const direction = ref<SyncRuleDirection>('aToB')
const deleteMode = ref<SyncRuleDeleteMode>(defaultDeleteMode('aToB'))
const minutes = ref(String(DEFAULT_INTERVAL_MINUTES))
const consent = ref(false)
const saving = ref(false)
const error = ref<string | null>(null)
const autoName = ref('')

/** A fresh dialog each time it opens, or the rule to change. */
watch(open, (isOpen) => {
  if (!isOpen) return
  const rule = props.rule
  name.value = rule?.name ?? ''
  placeA.value = rule ? { ...rule.placeA, encrypted: false } : null
  placeB.value = rule ? { ...rule.placeB, encrypted: false } : null
  direction.value = rule?.direction ?? 'aToB'
  deleteMode.value = rule?.deleteMode ?? defaultDeleteMode('aToB')
  minutes.value = rule
    ? String(Math.round(rule.intervalSecs / 60))
    : String(DEFAULT_INTERVAL_MINUTES)
  consent.value = false
  error.value = null
  autoName.value = ''
})

watch(direction, (next) => {
  // Back at the stored direction, the stored delete mode comes back with it.
  deleteMode.value =
    props.rule && next === props.rule.direction
      ? props.rule.deleteMode
      : defaultDeleteMode(next)
})

/** Whether `side` still holds the stored place of the rule changed. */
function keepsStored(side: Side): boolean {
  const rule = props.rule
  const place = placeOf(side)
  const stored = side === 'a' ? rule?.placeA : rule?.placeB
  return (
    !!place &&
    !!stored &&
    place.path === stored.path &&
    JSON.stringify(place.source) === JSON.stringify(stored.source)
  )
}

/** The name follows the source folder until the user types one. */
watch(placeA, (next) => {
  if (!next || keepsStored('a')) return
  if (name.value.trim() === '' || name.value === autoName.value) {
    autoName.value = lastName(next.path)
    name.value = autoName.value
  }
})

watch(name, (next) => {
  if (next !== autoName.value) autoName.value = ''
})

type Side = 'a' | 'b'

function placeOf(side: Side): PickedFolder | null {
  return side === 'a' ? placeA.value : placeB.value
}

function labelOf(side: Side): string {
  return t(side === 'a' ? 'files.syncRules.placeA' : 'files.syncRules.placeB')
}

/** The place copied into that would get files of an encrypted folder decrypted. */
const consentSide = computed(() =>
  targetSides(direction.value).find((side) =>
    needsDecryptConsent(
      placeOf(side === 'a' ? 'b' : 'a')?.encrypted ?? false,
      placeOf(side)?.encrypted ?? false,
    ),
  ),
)
/** The place copied into that is a storage without encryption. */
const cloudSide = computed(() =>
  targetSides(direction.value).find((side) => {
    const place = placeOf(side)
    return (
      !!place &&
      showsCloudHint(place.source.kind === 'storage', place.encrypted)
    )
  }),
)

const needsConsent = computed(() => consentSide.value !== undefined)
const cloudHint = computed(() => cloudSide.value !== undefined)
const cloudHintText = computed(() =>
  cloudSide.value
    ? t('files.syncRules.cloudHint', { place: labelOf(cloudSide.value) })
    : '',
)
const decryptConsentText = computed(() =>
  consentSide.value
    ? t('files.syncRules.decryptConsent', {
        place: labelOf(consentSide.value),
      })
    : '',
)

const directionOptions = computed(() =>
  (['aToB', 'bToA', 'both'] as const).map((value) => ({
    value,
    label: t(`files.syncRules.direction.${value}`),
  })),
)
const deleteOptions = computed(() =>
  (['keep', 'mirror', 'ask'] as const).map((mode) => ({
    value: mode,
    label: t(`files.syncRules.deleteMode.${mode}`),
  })),
)

const ready = computed(() =>
  canSave({
    name: name.value,
    hasA: !!placeA.value,
    hasB: !!placeB.value,
    needsConsent: needsConsent.value,
    consent: consent.value,
  }),
)

/** The rule as it stands in the dialog. Unchanged places keep the stored consent (FR-030a). */
function draft(): SyncRuleDraft | null {
  if (!placeA.value || !placeB.value) return null
  const rule = props.rule
  const unchanged = !!rule && keepsStored('a') && keepsStored('b')
  return {
    name: name.value.trim(),
    placeA: { source: placeA.value.source, path: placeA.value.path },
    placeB: { source: placeB.value.source, path: placeB.value.path },
    direction: direction.value,
    deleteMode: deleteMode.value,
    intervalSecs: intervalSecs(
      String(minutes.value).trim() === '' ? Number.NaN : Number(minutes.value),
    ),
    enabled: rule?.enabled ?? true,
    allowDecrypted:
      unchanged && rule
        ? rule.allowDecrypted
        : needsConsent.value && consent.value,
  }
}

/** The stored name of a place that is kept, else "Ort A" or "Ort B". */
const previewLabels = computed(() => ({
  a: keepsStored('a') && props.labels ? props.labels.a : labelOf('a'),
  b: keepsStored('b') && props.labels ? props.labels.b : labelOf('b'),
}))

function openPreview() {
  const next = draft()
  if (!next) return
  previewTarget.value = { id: props.rule?.id, draft: next }
  previewing.value = true
}

async function saveAsync() {
  const next = draft()
  if (!ready.value || saving.value || !next) return
  saving.value = true
  try {
    if (props.rule) await updateAsync(props.rule.id, next)
    else await createAsync(next)
    open.value = false
    emit('saved')
  } catch (cause) {
    const code = asSyncRuleError(cause)?.code
    error.value = code
      ? t(`files.syncRules.error.${code}`)
      : t('files.syncRules.failed')
  } finally {
    saving.value = false
  }
}
</script>

<template>
  <UiDrawerModal
    v-model:open="open"
    :title="
      rule ? t('files.syncRules.edit.title') : t('files.syncRules.create.title')
    "
  >
    <template #content>
      <form
        class="flex flex-col gap-4"
        data-testid="files-sync-rule-form"
        @submit.prevent="saveAsync"
      >
        <div class="flex flex-col gap-1">
          <span class="text-sm font-medium">{{
            t('files.syncRules.placeA')
          }}</span>
          <span
            v-if="keepsStored('a')"
            class="truncate text-xs text-muted-foreground"
            data-testid="files-sync-rule-stored-a"
            >{{
              t('files.syncRules.edit.stored', { place: labels?.a ?? '' })
            }}</span
          >
          <FilesFolderPicker v-model="placeA" :sources="sources" name="a" />
        </div>
        <div class="flex flex-col gap-1">
          <span class="text-sm font-medium">{{
            t('files.syncRules.placeB')
          }}</span>
          <span
            v-if="keepsStored('b')"
            class="truncate text-xs text-muted-foreground"
            data-testid="files-sync-rule-stored-b"
            >{{
              t('files.syncRules.edit.stored', { place: labels?.b ?? '' })
            }}</span
          >
          <FilesFolderPicker v-model="placeB" :sources="sources" name="b" />
        </div>
        <UiInput
          id="files-sync-rule-name"
          v-model="name"
          :label="t('files.syncRules.name')"
          :labels="fieldLabels.input.value"
          data-testid="files-sync-rule-name"
        />
        <div class="grid gap-3 sm:grid-cols-2">
          <label class="flex flex-col gap-1 text-sm">
            {{ t('files.syncRules.direction.label') }}
            <UiSelect
              v-model="direction"
              :options="directionOptions"
              :aria-label="t('files.syncRules.direction.label')"
              data-testid="files-sync-rule-direction"
            />
          </label>
          <label class="flex flex-col gap-1 text-sm">
            {{ t('files.syncRules.deleteMode.label') }}
            <UiSelect
              v-model="deleteMode"
              :options="deleteOptions"
              :aria-label="t('files.syncRules.deleteMode.label')"
              data-testid="files-sync-rule-delete-mode"
            />
          </label>
        </div>
        <UiInput
          id="files-sync-rule-interval"
          v-model="minutes"
          type="number"
          inputmode="numeric"
          :label="t('files.syncRules.interval')"
          :labels="fieldLabels.input.value"
          data-testid="files-sync-rule-interval"
        />
        <p
          v-if="cloudHint"
          class="text-xs text-muted-foreground"
          data-testid="files-sync-rule-cloud-hint"
        >
          {{ cloudHintText }}
        </p>
        <label
          v-if="needsConsent"
          class="flex items-start gap-2 text-sm"
          data-testid="files-sync-rule-consent"
        >
          <ShadcnCheckbox
            v-model="consent"
            data-testid="files-sync-rule-consent-check"
          />
          <span>{{ decryptConsentText }}</span>
        </label>
        <p
          v-if="error"
          class="text-sm text-destructive"
          role="alert"
          data-testid="files-sync-rule-error"
        >
          {{ error }}
        </p>
        <div class="flex justify-end gap-2">
          <UiButton type="button" variant="outline" @click="open = false">{{
            t('files.cancel')
          }}</UiButton>
          <UiButton
            type="button"
            variant="outline"
            :disabled="!placeA || !placeB"
            data-testid="files-sync-rule-preview"
            @click="openPreview"
            >{{ t('files.syncRules.plan.open') }}</UiButton
          >
          <UiButton
            type="submit"
            :disabled="!ready"
            :loading="saving"
            data-testid="files-sync-rule-save"
            >{{
              rule
                ? t('files.syncRules.edit.save')
                : t('files.syncRules.create.save')
            }}</UiButton
          >
        </div>
      </form>
    </template>
  </UiDrawerModal>
  <FilesSyncRulePlan
    v-model:open="previewing"
    :target="previewTarget"
    :labels="previewLabels"
  />
</template>
