<script setup lang="ts">
import { toast } from 'vue-sonner'
import type { DeviceInfo } from '~/composables/useDevice'
import type { TierRecommendation } from '~/composables/useCatalog'
import type { SttCatalogEntry } from '~/composables/useSttCatalog'
import { spokenLanguage } from '~/lib/assistant/settings'
import {
  defaultModel,
  type SpeechModelOption,
} from '~/lib/assistant/speechModel'

// Onboarding wizard for spec 002 US1, extended by spec 010 with a third
// step (speech-to-text model choice) and by spec 081 with a fourth (the voice:
// the language's Pocket TTS model, so the assistant speaks right away). The middleware only sends us here
// while `alias` is still `null` on this device. We keep the entered alias
// local until the final step commits (a chosen/skipped chat model,
// followed by a chosen/skipped STT model); only then do we persist the
// alias and route to /workspace/[instance]. An exit between steps
// therefore leaves the vault in "still onboarding" state and the wizard
// re-opens next launch.
const route = useRoute()
const { t, locale } = useI18n()
const { errString } = useErrorString()
const { currentDeviceInfoAsync, updateDeviceAliasAsync } = useDevice()
const { recommendTiersAsync } = useCatalog()
const { downloadFromCatalogAsync } = useModels()
const { recommendTiersAsync: recommendSttTiersAsync } = useSttCatalog()
const { downloadFromCatalogAsync: downloadSttFromCatalogAsync } = useSttModels()
const { setPrefAsync } = usePreferences()
const { activateLocalModelAsync } = useChat()
const { speechModelsAsync, downloadModelAsync: downloadVoiceAsync } =
  useTtsVoices()
const downloads = useModelDownloadsStore()

const instanceName = computed(() => {
  const raw = route.params.instance
  return typeof raw === 'string'
    ? raw
    : Array.isArray(raw)
      ? (raw[0] ?? '')
      : ''
})

const step = ref<'alias' | 'model' | 'sttModel' | 'voice'>('alias')
const deviceInfo = ref<DeviceInfo | null>(null)
const alias = ref('')
const tiers = ref<TierRecommendation[]>([])
const downloadingId = ref<string | null>(null)
const downloadError = ref<string | null>(null)
const sttTiers = ref<TierRecommendation<SttCatalogEntry>[]>([])
const sttDownloadingId = ref<string | null>(null)
const sttDownloadError = ref<string | null>(null)
/** The speech model offered in the voice step: the default of the interface language. */
const voiceLanguage = computed(() => spokenLanguage('ui', locale.value))
const voiceOption = ref<SpeechModelOption | null>(null)
const voiceDownloading = ref(false)
const voiceDownloadError = ref<string | null>(null)
let voiceOptionsLoaded: Promise<void> = Promise.resolve()
/** The device info could not be read (its error went out as a toast); no loading notice then. */
const loadFailed = ref(false)

onMounted(async () => {
  // Start this before the other catalog requests: the operator can reach and skip the STT step
  // while those are still loading, but must not bypass the offered voice by clicking quickly.
  voiceOptionsLoaded = loadVoiceOptionAsync()
  try {
    deviceInfo.value = await currentDeviceInfoAsync()
    alias.value =
      deviceInfo.value?.alias ??
      deviceInfo.value?.hostname ??
      t('onboarding.alias.defaultPlaceholder')
  } catch (e) {
    loadFailed.value = true
    toast.error(`${t('errors.deviceInfoFailed')}: ${errString(e)}`)
    return
  }
  try {
    tiers.value = await recommendTiersAsync()
  } catch (e) {
    // Empty catalog only — the model step still lets the operator skip.
    tiers.value = []
    downloadError.value = errString(e)
  }
  try {
    sttTiers.value = await recommendSttTiersAsync()
  } catch (e) {
    sttTiers.value = []
    sttDownloadError.value = errString(e)
  }
})

async function loadVoiceOptionAsync() {
  try {
    await downloads.watchDownloads()
    voiceOption.value =
      defaultModel(await speechModelsAsync(voiceLanguage.value)) ?? null
  } catch (e) {
    // The voice step offers nothing then; the settings can still load a voice.
    voiceOption.value = null
    voiceDownloadError.value = errString(e)
  }
}

/** After the speech recognition step: the voice, unless it is here already. */
async function afterSttAsync() {
  await voiceOptionsLoaded
  if (voiceOption.value && !voiceOption.value.installed) {
    step.value = 'voice'
    return
  }
  await finishOnboardingAsync()
}

async function completeVoiceAsync() {
  const option = voiceOption.value
  if (!option) return
  voiceDownloading.value = true
  voiceDownloadError.value = null
  try {
    // Pocket TTS is the default speech model (spec 081 FR-012): no preference to write.
    // action-exempt: onboarding runs before the window manager (spec 002), outside the app catalog.
    await downloadVoiceAsync(option.id, voiceLanguage.value)
    await finishOnboardingAsync()
  } catch (e) {
    voiceDownloadError.value = errString(e)
  } finally {
    voiceDownloading.value = false
    downloads.clearDownload(option.id)
  }
}

async function finalizeAliasAsync() {
  const trimmed = alias.value.trim()
  if (!trimmed) {
    throw new Error(t('onboarding.alias.required'))
  }
  // action-exempt: onboarding runs before the window manager (spec 002), outside the app catalog.
  await updateDeviceAliasAsync(trimmed)
}

async function finishOnboardingAsync() {
  try {
    await finalizeAliasAsync()
  } catch (e) {
    toast.error(`${t('errors.deviceInfoFailed')}: ${errString(e)}`)
    return
  }
  await navigateTo(`/workspace/${encodeURIComponent(instanceName.value)}`, {
    replace: true,
  })
}

async function completeWithModel(rec: TierRecommendation) {
  const info = deviceInfo.value
  if (!info) {
    return
  }
  downloadingId.value = rec.entry.id
  downloadError.value = null
  try {
    // action-exempt: onboarding runs before the window manager (spec 002), outside the app catalog.
    const installed = await downloadFromCatalogAsync(rec.entry.id)
    // The wizard's explicit choice is the model a new chat starts with: the last picked one
    // (spec 002 FR-007, FR-009 as amended 2026-11-02). There is no default model.
    // action-exempt: onboarding runs before the window manager (spec 002), outside the app catalog.
    await setPrefAsync(
      { kind: 'device', uuid: info.vaultDeviceUuid },
      'chat.last_active_model_id',
      installed.id,
    )
    // The model goes into memory in the background and stays there as the active local model, so
    // the first chat can use it. A failure holds nothing up: the settings activate it again.
    // action-exempt: onboarding runs before the window manager (spec 002), outside the app catalog.
    void activateLocalModelAsync(installed.id).catch((e: unknown) => {
      console.error('[onboarding] activating the model failed', e)
    })
    step.value = 'sttModel'
  } catch (e) {
    downloadError.value = errString(e)
  } finally {
    downloadingId.value = null
  }
}

async function completeSttWithModel(rec: TierRecommendation<SttCatalogEntry>) {
  const info = deviceInfo.value
  if (!info) {
    return
  }
  sttDownloadingId.value = rec.entry.id
  sttDownloadError.value = null
  try {
    const installed = await downloadSttFromCatalogAsync(rec.entry.id)
    // Device-scoped only — no vault-wide variant (data-model.md), the
    // transcription hardware is tied to this device.
    // action-exempt: onboarding runs before the window manager (spec 002), outside the app catalog.
    await setPrefAsync(
      { kind: 'device', uuid: info.vaultDeviceUuid },
      'voice.stt_model_id',
      installed.id,
    )
    await afterSttAsync()
  } catch (e) {
    sttDownloadError.value = errString(e)
  } finally {
    sttDownloadingId.value = null
  }
}
</script>

<template>
  <main
    class="min-h-screen flex flex-col items-center justify-center gap-6 p-6"
  >
    <div class="w-full max-w-xl flex flex-col gap-4">
      <div>
        <h1 class="text-2xl font-semibold">
          {{ t('onboarding.wizard.title') }}
        </h1>
        <p class="text-sm text-muted-foreground">
          {{ t('onboarding.wizard.subtitle') }}
        </p>
      </div>

      <div
        v-if="!deviceInfo && !loadFailed"
        class="text-sm text-muted-foreground"
      >
        {{ t('onboarding.wizard.loadingDeviceInfo') }}
      </div>

      <template v-else-if="deviceInfo">
        <OnboardingAliasStep
          v-if="step === 'alias'"
          v-model="alias"
          :hostname-hint="deviceInfo.hostname"
          @next="step = 'model'"
        />
        <OnboardingModelChoiceStep
          v-else-if="step === 'model'"
          :tiers="tiers"
          :downloading-id="downloadingId"
          :download-error="downloadError"
          @choose="completeWithModel"
          @skip="step = 'sttModel'"
          @back="step = 'alias'"
        />
        <OnboardingSttModelChoiceStep
          v-else-if="step === 'sttModel'"
          :tiers="sttTiers"
          :downloading-id="sttDownloadingId"
          :download-error="sttDownloadError"
          @choose="completeSttWithModel"
          @skip="afterSttAsync"
          @back="step = 'model'"
        />
        <OnboardingVoiceStep
          v-else
          :option="voiceOption"
          :downloading="voiceDownloading"
          :download-error="voiceDownloadError"
          @choose="completeVoiceAsync"
          @skip="finishOnboardingAsync"
          @back="step = 'sttModel'"
        />
      </template>
    </div>
  </main>
</template>
