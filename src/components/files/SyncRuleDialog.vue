<script setup lang="ts">
/**
 * Creates a sync rule (spec 061 US1, US2, US4 scenario 1, FR-001, FR-005, FR-030a, FR-031): a name,
 * two places from the folder picker, the direction (one way or both ways), the delete mode and the
 * interval. The dialog asks for consent when files would leave an encrypted folder for a plain one,
 * and points to encrypted folders when a place copied into is a storage without one.
 */
import type { Sources } from '@bindings/Sources'
import type { SyncRuleDeleteMode } from '@bindings/SyncRuleDeleteMode'
import type { SyncRuleDirection } from '@bindings/SyncRuleDirection'
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

defineProps<{
  sources: Sources | null
}>()

const open = defineModel<boolean>('open', { required: true })
const emit = defineEmits<{ saved: [] }>()

const { t } = useI18n()
const fieldLabels = useFieldLabels()
const { createAsync } = useSyncRules()

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

/** A fresh dialog each time it opens. */
watch(open, (isOpen) => {
  if (!isOpen) return
  name.value = ''
  placeA.value = null
  placeB.value = null
  direction.value = 'aToB'
  deleteMode.value = defaultDeleteMode('aToB')
  minutes.value = String(DEFAULT_INTERVAL_MINUTES)
  consent.value = false
  error.value = null
  autoName.value = ''
})

watch(direction, (next) => {
  deleteMode.value = defaultDeleteMode(next)
})

/** The name follows the source folder until the user types one. */
watch(placeA, (next) => {
  if (!next) return
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

async function saveAsync() {
  if (!ready.value || saving.value || !placeA.value || !placeB.value) return
  saving.value = true
  try {
    await createAsync({
      name: name.value.trim(),
      placeA: { source: placeA.value.source, path: placeA.value.path },
      placeB: { source: placeB.value.source, path: placeB.value.path },
      direction: direction.value,
      deleteMode: deleteMode.value,
      intervalSecs: intervalSecs(
        String(minutes.value).trim() === ''
          ? Number.NaN
          : Number(minutes.value),
      ),
      enabled: true,
      allowDecrypted: needsConsent.value && consent.value,
    })
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
  <UiDrawerModal v-model:open="open" :title="t('files.syncRules.create.title')">
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
          <FilesFolderPicker v-model="placeA" :sources="sources" name="a" />
        </div>
        <div class="flex flex-col gap-1">
          <span class="text-sm font-medium">{{
            t('files.syncRules.placeB')
          }}</span>
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
            type="submit"
            :disabled="!ready"
            :loading="saving"
            data-testid="files-sync-rule-save"
            >{{ t('files.syncRules.create.save') }}</UiButton
          >
        </div>
      </form>
    </template>
  </UiDrawerModal>
</template>
