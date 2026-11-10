<script setup lang="ts">
/**
 * Creates or changes a sync rule (spec 061 US1, US2, US4 scenarios 1, 4 and 6, FR-001, FR-005,
 * FR-030a, FR-031): a name, the source and the target from the folder picker, the direction (from
 * the source to the target, or both ways),
 * the delete mode and the interval. The source may be a folder or one file (spec 074); a file rule
 * deletes nothing, so the dialog does not ask for a delete mode then, and an edit may switch the
 * source between a folder and a file. The dialog asks for consent when files would leave an encrypted
 * folder for a plain one, and points to encrypted folders when a place copied into is a storage
 * without one. The preview shows the plan of the rule as it stands in the dialog before it is saved.
 * A changed rule keeps its places until another folder is picked; the picker cannot open at a
 * stored place. A new rule may start with a source place (`initialSource`, spec 073), which the
 * folder picker replaces like any other.
 */
import type { Sources } from '@bindings/Sources'
import type { SyncRuleDeleteMode } from '@bindings/SyncRuleDeleteMode'
import type { SyncRuleDirection } from '@bindings/SyncRuleDirection'
import type { SyncRule } from '@bindings/SyncRule'
import type { SyncRulePlace } from '@bindings/SyncRulePlace'
import type { SyncRuleDraft } from '@bindings/SyncRuleDraft'
import { toast } from 'vue-sonner'
import { asSyncRuleError } from '~/composables/useSyncRules'
import {
  canSave,
  DEFAULT_INTERVAL_MINUTES,
  defaultDeleteMode,
  intervalSecs,
  lastName,
  needsDecryptConsent,
  showsCloudHint,
  showsDeleteMode,
  targetSides,
  type SourceKind,
} from '~/lib/files/syncRules'
import type { PickedFolder } from './FolderPicker.vue'

const props = defineProps<{
  sources: Sources | null
  /** The rule to change; `null` for a new one. */
  rule?: SyncRule | null
  /** The names of both places of the rule changed. */
  labels?: { source: string; target: string }
  /** The source place a new rule starts with (spec 073: the file browser's menu); `null` for none. */
  initialSource?: PickedFolder | null
}>()

const open = defineModel<boolean>('open', { required: true })
/** `saved` carries the name the rule was saved under. */
const emit = defineEmits<{ saved: [name: string] }>()

const { t } = useI18n()
const fieldLabels = useFieldLabels()
const { createAsync, updateAsync } = useSyncRules()
const previewing = ref(false)
const previewTarget = ref<{ id?: string; draft?: SyncRuleDraft } | null>(null)

const name = ref('')
const source = ref<PickedFolder | null>(null)
const target = ref<PickedFolder | null>(null)
const direction = ref<SyncRuleDirection>('sourceToTarget')
const deleteMode = ref<SyncRuleDeleteMode>(defaultDeleteMode('sourceToTarget'))
const minutes = ref(String(DEFAULT_INTERVAL_MINUTES))
const consent = ref(false)
const saving = ref(false)
const autoName = ref('')

/** A fresh dialog each time it opens, or the rule to change. */
watch(open, (isOpen) => {
  if (!isOpen) return
  const rule = props.rule
  name.value = rule?.name ?? ''
  source.value = rule
    ? pickedOf(rule.source, rule.sourceKind)
    : props.initialSource
      ? { ...props.initialSource }
      : null
  target.value = rule ? pickedOf(rule.target, 'folder') : null
  direction.value = rule?.direction ?? 'sourceToTarget'
  deleteMode.value = rule?.deleteMode ?? defaultDeleteMode('sourceToTarget')
  minutes.value = rule
    ? String(Math.round(rule.intervalSecs / 60))
    : String(DEFAULT_INTERVAL_MINUTES)
  consent.value = false
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
  const stored = side === 'source' ? rule?.source : rule?.target
  const sameKind = side === 'target' || place?.kind === rule?.sourceKind
  return (
    !!place &&
    !!stored &&
    sameKind &&
    place.path === stored.path &&
    JSON.stringify(place.source) === JSON.stringify(stored.location)
  )
}

/** The name follows the source folder until the user types one. */
watch(source, (next) => {
  if (!next || keepsStored('source')) return
  if (name.value.trim() === '' || name.value === autoName.value) {
    autoName.value = lastName(next.path)
    name.value = autoName.value
  }
})

watch(name, (next) => {
  if (next !== autoName.value) autoName.value = ''
})

type Side = 'source' | 'target'

function placeOf(side: Side): PickedFolder | null {
  return side === 'source' ? source.value : target.value
}

/** The folder picker's value for a stored place: its storage reference is `source` there. */
function pickedOf(place: SyncRulePlace, kind: SourceKind): PickedFolder {
  return {
    source: place.location,
    path: place.path,
    encrypted: false,
    kind,
  }
}

function labelOf(side: Side): string {
  return t(
    side === 'source' ? 'files.syncRules.source' : 'files.syncRules.target',
  )
}

/** The place copied into that would get files of an encrypted folder decrypted. */
const consentSide = computed(() =>
  targetSides(direction.value).find((side) =>
    needsDecryptConsent(
      placeOf(side === 'source' ? 'target' : 'source')?.encrypted ?? false,
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
  (['sourceToTarget', 'both'] as const).map((value) => ({
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
    hasSource: !!source.value,
    hasTarget: !!target.value,
    needsConsent: needsConsent.value,
    consent: consent.value,
  }),
)

/** The rule as it stands in the dialog. Unchanged places keep the stored consent (FR-030a). */
function draft(): SyncRuleDraft | null {
  if (!source.value || !target.value) return null
  const rule = props.rule
  const unchanged = !!rule && keepsStored('source') && keepsStored('target')
  return {
    name: name.value.trim(),
    source: { location: source.value.source, path: source.value.path },
    target: { location: target.value.source, path: target.value.path },
    sourceKind: source.value.kind,
    direction: direction.value,
    deleteMode: showsDeleteMode(source.value.kind) ? deleteMode.value : 'keep',
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

/** The stored name of a place that is kept, else "Quelle" or "Ziel". */
const previewLabels = computed(() => ({
  source:
    keepsStored('source') && props.labels
      ? props.labels.source
      : labelOf('source'),
  target:
    keepsStored('target') && props.labels
      ? props.labels.target
      : labelOf('target'),
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
    emit('saved', next.name)
  } catch (cause) {
    const code = asSyncRuleError(cause)?.code
    toast.error(
      code ? t(`files.syncRules.error.${code}`) : t('files.syncRules.failed'),
    )
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
        <UiInput
          id="files-sync-rule-name"
          v-model="name"
          :label="t('files.syncRules.name')"
          :labels="fieldLabels.input.value"
          data-testid="files-sync-rule-name"
        />
        <div class="flex flex-col gap-1">
          <span class="text-sm font-medium">{{
            t('files.syncRules.source')
          }}</span>
          <FilesFolderPicker
            v-model="source"
            :sources="sources"
            name="source"
            allow-files
          />
        </div>
        <div class="flex flex-col gap-1">
          <span class="text-sm font-medium">{{
            t('files.syncRules.target')
          }}</span>
          <FilesFolderPicker
            v-model="target"
            :sources="sources"
            name="target"
          />
        </div>
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
          <label
            v-if="showsDeleteMode(source?.kind ?? 'folder')"
            class="flex flex-col gap-1 text-sm"
          >
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
        <div class="flex justify-end gap-2">
          <UiButton type="button" variant="outline" @click="open = false">{{
            t('files.cancel')
          }}</UiButton>
          <UiButton
            type="button"
            variant="outline"
            :disabled="!source || !target"
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
