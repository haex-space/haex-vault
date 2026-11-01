<script setup lang="ts">
/**
 * "Töne" under KI → Sprache (spec 066 T030, FR-043–FR-045): the tones on or off, their volume
 * apart from the speech, and every tone with what it means and an "Anhören". Device preferences
 * `assistant.tones.enabled` and `assistant.tones.volume`, saved on selection (spec 023 FR-021).
 * "Hört zu" and "Geheimnis" sound even with tones off; the page says so at both.
 */
import {
  ALWAYS_AUDIBLE_TONES,
  assistantLabelKey,
  parseTonesEnabled,
  parseTonesVolume,
  TONE_IDS,
  TONES_ENABLED_KEY,
  TONES_VOLUME,
  TONES_VOLUME_KEY,
  type ToneId,
} from '~/lib/assistant/settings'
import { useSettingsDevice } from '~/components/settings/deviceContext'

const { t } = useI18n()
const { errString } = useErrorString()
const { getPrefAsync, setPrefAsync } = usePreferences()
const { tonePreviewAsync } = useTtsVoices()
const device = useSettingsDevice()

const scope = computed(() => ({
  kind: 'device' as const,
  uuid: device.info.value.vaultDeviceUuid,
}))

const enabled = ref(true)
const volume = ref<number>(TONES_VOLUME.default)
const loading = ref(true)
const loadError = ref<string | null>(null)
const saving = ref(false)
const savedFlash = ref(false)
const opError = ref<string | null>(null)
/** The tone whose sample sounds. */
const playing = ref<ToneId | null>(null)

const volumeLabel = computed(() =>
  t('settings.tones.volumeValue', { volume: volume.value }),
)

function alwaysAudible(tone: ToneId): boolean {
  return ALWAYS_AUDIBLE_TONES.includes(tone)
}

function description(tone: ToneId): string {
  const when = t(`settings.tones.when.${tone}`)
  return alwaysAudible(tone) ? `${when} ${t('settings.tones.always')}` : when
}

/** A `quiet` reload (after a change from elsewhere) keeps the shown values until the new ones are read. */
async function reloadAsync(quiet = false) {
  if (!quiet) loading.value = true
  loadError.value = null
  try {
    const [storedEnabled, storedVolume] = await Promise.all([
      getPrefAsync(scope.value, TONES_ENABLED_KEY),
      getPrefAsync(scope.value, TONES_VOLUME_KEY),
    ])
    enabled.value = parseTonesEnabled(storedEnabled)
    volume.value = parseTonesVolume(storedVolume)
  } catch (e) {
    loadError.value = errString(e)
  } finally {
    loading.value = false
  }
}

async function saveAsync(key: string, value: string) {
  saving.value = true
  savedFlash.value = false
  opError.value = null
  try {
    // action-exempt: how the assistant sounds is this device's own choice; spec 066 gives agents no action for it.
    await setPrefAsync(scope.value, key, value)
    savedFlash.value = true
  } catch (e) {
    opError.value = t('settings.tones.saveFailed', { reason: errString(e) })
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

async function playAsync(tone: ToneId) {
  playing.value = tone
  opError.value = null
  try {
    await tonePreviewAsync(tone, volume.value)
  } catch (e) {
    opError.value = t('settings.tones.previewFailed', { reason: errString(e) })
  } finally {
    if (playing.value === tone) playing.value = null
  }
}

onMounted(() => reloadAsync())
onVaultTablesChanged(['preferences'], () => {
  if (!saving.value) return reloadAsync(true)
})
</script>

<template>
  <section class="flex flex-col gap-6">
    <div v-if="loading" class="text-sm text-muted-foreground">
      {{ t('onboarding.wizard.loadingDeviceInfo') }}
    </div>

    <p v-if="loadError" class="text-sm text-destructive" role="alert">
      {{ t('errors.prefLoadFailed') }}: {{ loadError }}
    </p>

    <template v-if="!loading && !loadError">
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
        </SettingsRow>
      </SettingsGroup>

      <p v-if="savedFlash" class="px-1 text-xs text-success" role="status">
        {{ t('settings.tones.saved') }}
      </p>
      <p v-if="opError" class="px-1 text-xs text-destructive" role="alert">
        {{ opError }}
      </p>
    </template>
  </section>
</template>
