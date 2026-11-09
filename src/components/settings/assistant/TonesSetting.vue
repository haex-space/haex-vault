<script setup lang="ts">
/**
 * "Töne" under KI → Sprache (spec 066 T030, T159, FR-043–FR-045a): the tones on or off, their
 * volume apart from the speech, and every tone with what it means, which sound it uses on this
 * device (built-in or an own tone) and an "Anhören". Own tones are imported once and listed below;
 * they exist on every device, the choice per occasion is this device's. Device preferences
 * `assistant.tones.enabled`, `assistant.tones.volume` and (through the backend)
 * `assistant.tones.custom`, saved on selection (spec 023 FR-021). "Hört zu" and "Geheimnis" sound
 * even with tones off; the page says so at both.
 */
import { toast } from 'vue-sonner'
import {
  ALWAYS_AUDIBLE_TONES,
  assistantLabelKey,
  parseTonesEnabled,
  parseTonesVolume,
  TONE_FILE_EXTENSIONS,
  TONE_IDS,
  TONES_ENABLED_KEY,
  TONES_VOLUME,
  TONES_VOLUME_KEY,
  type ToneId,
} from '~/lib/assistant/settings'
import { useSettingsDevice } from '~/components/settings/deviceContext'
import type { CustomTone } from '~/composables/useTtsVoices'

const { t } = useI18n()
const { errString } = useErrorString()
const { getPrefAsync, setPrefAsync } = usePreferences()
const {
  tonePreviewAsync,
  tonesListAsync,
  toneImportAsync,
  toneAssignAsync,
  toneDeleteAsync,
} = useTtsVoices()
const { pickOneAsync } = usePickedFile()
const device = useSettingsDevice()

const scope = computed(() => ({
  kind: 'device' as const,
  uuid: device.info.value.vaultDeviceUuid,
}))

const enabled = ref(true)
const volume = ref<number>(TONES_VOLUME.default)
const loading = ref(true)
/** The settings could not be read (shown as a toast); the page then shows no controls. */
const loadFailed = ref(false)
const saving = ref(false)
/** The tone (or own tone id) whose sample sounds. */
const playing = ref<string | null>(null)
const custom = ref<CustomTone[]>([])
const assigned = ref<Partial<Record<ToneId, string>>>({})
const importing = ref(false)
/** The own tone whose delete waits for a second click. */
const confirmingDelete = ref<string | null>(null)

/** Select value for "pick a file and use it here". */
const PICK = '__pick__'

const soundOptions = computed(() => [
  { value: '', label: t('settings.tones.builtIn') },
  ...custom.value.map((tone) => ({ value: tone.id, label: tone.name })),
  { value: PICK, label: t('settings.tones.pickFile') },
])

function seconds(tone: CustomTone): string {
  return t('settings.tones.duration', {
    seconds: (tone.durationMs / 1000).toLocaleString(undefined, {
      maximumFractionDigits: 1,
    }),
  })
}

/** A backend refusal (`tooLarge`, `unsupported`, `silent`, `tooLong`, `unknownTone`) in words. */
function reasonText(e: unknown, tone?: ToneId): string {
  const reason = errString(e)
  const known = ['tooLarge', 'unsupported', 'silent', 'tooLong', 'unknownTone']
  const key = known.find((k) => reason.includes(k))
  if (!key) return reason
  if (key === 'tooLong' && tone === 'working')
    return t('settings.tones.reasons.tooLongWorking')
  return t(`settings.tones.reasons.${key}`)
}

const volumeLabel = computed(() =>
  t('settings.tones.volumeValue', { volume: volume.value }),
)

function alwaysAudible(tone: ToneId): boolean {
  return ALWAYS_AUDIBLE_TONES.includes(tone)
}

function isVoiceDisabledError(e: unknown): boolean {
  return (
    typeof e === 'object' &&
    e !== null &&
    'kind' in e &&
    e.kind === 'InvalidInput' &&
    'reason' in e &&
    e.reason === 'the voice assistant is not enabled in this build'
  )
}

function description(tone: ToneId): string {
  const when = t(`settings.tones.when.${tone}`)
  return alwaysAudible(tone) ? `${when} ${t('settings.tones.always')}` : when
}

/** A `quiet` reload (after a change from elsewhere) keeps the shown values until the new ones are read. */
async function reloadAsync(quiet = false) {
  if (!quiet) loading.value = true
  loadFailed.value = false
  try {
    const [storedEnabled, storedVolume] = await Promise.all([
      getPrefAsync(scope.value, TONES_ENABLED_KEY),
      getPrefAsync(scope.value, TONES_VOLUME_KEY),
    ])
    enabled.value = parseTonesEnabled(storedEnabled)
    volume.value = parseTonesVolume(storedVolume)
    try {
      const overview = await tonesListAsync()
      custom.value = overview.custom
      assigned.value = overview.assigned
    } catch (e) {
      if (!isVoiceDisabledError(e)) throw e
      custom.value = []
      assigned.value = {}
    }
  } catch (e) {
    loadFailed.value = true
    toast.error(`${t('errors.prefLoadFailed')}: ${errString(e)}`)
  } finally {
    loading.value = false
  }
}

async function saveAsync(key: string, value: string) {
  saving.value = true
  try {
    // action-exempt: how the assistant sounds is this device's own choice; spec 066 gives agents no action for it.
    await setPrefAsync(scope.value, key, value)
  } catch (e) {
    toast.error(t('settings.tones.saveFailed', { reason: errString(e) }))
  } finally {
    saving.value = false
  }
}

async function toggleAsync(next: boolean) {
  enabled.value = next
  await saveAsync(TONES_ENABLED_KEY, String(next))
}

async function saveVolumeAsync() {
  await saveAsync(TONES_VOLUME_KEY, String(volume.value))
}

async function playAsync(tone: ToneId, customId: string | null = null) {
  const key = customId ?? tone
  playing.value = key
  try {
    await tonePreviewAsync(tone, volume.value, customId)
  } catch (e) {
    toast.error(t('settings.tones.previewFailed', { reason: errString(e) }))
  } finally {
    if (playing.value === key) playing.value = null
  }
}

/** Picks an audio file and imports it; returns the new tone, or `null` when nothing was added. */
async function importAsync(): Promise<CustomTone | null> {
  const file = await pickOneAsync([
    {
      name: t('settings.tones.fileTypes'),
      extensions: [...TONE_FILE_EXTENSIONS],
    },
  ])
  if (!file) return null
  importing.value = true
  try {
    const tone = await toneImportAsync(file)
    custom.value = [...custom.value, tone].sort((a, b) =>
      a.name.localeCompare(b.name),
    )
    return tone
  } catch (e) {
    toast.error(t('settings.tones.importFailed', { reason: reasonText(e) }))
    return null
  } finally {
    importing.value = false
  }
}

async function chooseAsync(tone: ToneId, value: string) {
  let customId: string | null = value === '' ? null : value
  if (value === PICK) {
    const added = await importAsync()
    if (!added) return
    customId = added.id
  }
  saving.value = true
  try {
    // action-exempt: how the assistant sounds is this device's own choice; spec 066 gives agents no action for it.
    await toneAssignAsync(tone, customId)
    const rest = Object.fromEntries(
      Object.entries(assigned.value).filter(([occasion]) => occasion !== tone),
    )
    assigned.value = customId ? { ...rest, [tone]: customId } : rest
  } catch (e) {
    toast.error(
      t('settings.tones.saveFailed', {
        reason: reasonText(e, tone),
      }),
    )
  } finally {
    saving.value = false
  }
}

async function removeToneAsync(tone: CustomTone) {
  if (confirmingDelete.value !== tone.id) {
    confirmingDelete.value = tone.id
    return
  }
  confirmingDelete.value = null
  try {
    // action-exempt: removing an own tone is a settings-page choice; spec 066 gives agents no action for it.
    await toneDeleteAsync(tone.id)
    custom.value = custom.value.filter((c) => c.id !== tone.id)
    assigned.value = Object.fromEntries(
      Object.entries(assigned.value).filter(([, id]) => id !== tone.id),
    )
  } catch (e) {
    toast.error(t('settings.tones.saveFailed', { reason: reasonText(e) }))
  }
}

onMounted(() => reloadAsync())
onVaultTablesChanged(['preferences', 'haex_assistant_tones'], () => {
  if (!saving.value) return reloadAsync(true)
})
</script>

<template>
  <section class="flex flex-col gap-6">
    <div v-if="loading" class="text-sm text-muted-foreground">
      {{ t('onboarding.wizard.loadingDeviceInfo') }}
    </div>

    <template v-if="!loading && !loadFailed">
      <SettingsGroup>
        <SettingsRow
          :title="t(assistantLabelKey('assistant.tones.enabled'))"
          :description="t('settings.tones.enabledHint')"
          label-for="settings-tones-enabled"
        >
          <ShadcnSwitch
            id="settings-tones-enabled"
            :model-value="enabled"
            :disabled="saving"
            data-testid="settings-tones-enabled"
            @update:model-value="toggleAsync($event === true)"
          />
        </SettingsRow>
        <SettingsRow
          :title="t(assistantLabelKey('assistant.tones.volume'))"
          :description="t('settings.deviceOnly')"
          label-for="settings-tones-volume"
        >
          <span class="flex items-center gap-3">
            <input
              id="settings-tones-volume"
              v-model.number="volume"
              type="range"
              :min="TONES_VOLUME.min"
              :max="TONES_VOLUME.max"
              :step="TONES_VOLUME.step"
              :aria-valuetext="volumeLabel"
              class="w-36 accent-primary"
              data-testid="settings-tones-volume"
              @change="saveVolumeAsync"
            />
            <span class="w-12 text-right text-sm tabular-nums">
              {{ volumeLabel }}
            </span>
          </span>
        </SettingsRow>
      </SettingsGroup>

      <SettingsGroup :label="t('settings.tones.listLabel')">
        <SettingsRow
          v-for="tone in TONE_IDS"
          :key="tone"
          :title="t(`settings.tones.names.${tone}`)"
          :description="description(tone)"
          :data-testid="`settings-tone-${tone}`"
        >
          <span class="flex flex-wrap items-center justify-end gap-2">
            <SettingsSelect
              :id="`settings-tone-sound-${tone}`"
              :model-value="assigned[tone] ?? ''"
              :options="soundOptions"
              :disabled="saving || importing"
              :aria-label="
                t('settings.tones.soundFor', {
                  name: t(`settings.tones.names.${tone}`),
                })
              "
              class="w-44"
              :data-testid="`settings-tone-sound-${tone}`"
              @update:model-value="chooseAsync(tone, $event)"
            />
            <UiButton
              type="button"
              variant="outline"
              size="sm"
              :loading="playing === tone"
              :aria-label="
                t('settings.tones.listenTo', {
                  name: t(`settings.tones.names.${tone}`),
                })
              "
              :data-testid="`settings-tone-play-${tone}`"
              @click="playAsync(tone)"
            >
              {{ t('settings.tones.listen') }}
            </UiButton>
          </span>
        </SettingsRow>
      </SettingsGroup>

      <SettingsGroup :label="t('settings.tones.custom')">
        <SettingsRow
          v-for="tone in custom"
          :key="tone.id"
          :title="tone.name"
          :description="seconds(tone)"
          :data-testid="`settings-custom-tone-${tone.id}`"
        >
          <span class="flex flex-wrap items-center justify-end gap-2">
            <UiButton
              type="button"
              variant="outline"
              size="sm"
              :loading="playing === tone.id"
              :aria-label="t('settings.tones.listenTo', { name: tone.name })"
              @click="playAsync('listening', tone.id)"
            >
              {{ t('settings.tones.listen') }}
            </UiButton>
            <UiButton
              type="button"
              :variant="
                confirmingDelete === tone.id ? 'destructive' : 'outline'
              "
              size="sm"
              :aria-label="t('settings.tones.deleteOf', { name: tone.name })"
              :data-testid="`settings-custom-tone-delete-${tone.id}`"
              @click="removeToneAsync(tone)"
            >
              {{
                confirmingDelete === tone.id
                  ? t('settings.tones.deleteConfirm')
                  : t('settings.tones.delete')
              }}
            </UiButton>
          </span>
        </SettingsRow>
        <SettingsRow
          :title="custom.length === 0 ? t('settings.tones.noCustom') : ''"
          :description="t('settings.tones.customHint')"
        >
          <UiButton
            type="button"
            variant="outline"
            size="sm"
            :loading="importing"
            data-testid="settings-custom-tone-add"
            @click="importAsync"
          >
            {{ t('settings.tones.addFile') }}
          </UiButton>
        </SettingsRow>
      </SettingsGroup>
    </template>
  </section>
</template>
