<script setup lang="ts">
import type { DeviceInfo } from '~/composables/useDevice'
import type { TierRecommendation } from '~/composables/useCatalog'
import type { SttCatalogEntry } from '~/composables/useSttCatalog'

// Onboarding wizard for spec 002 US1, extended by spec 010 with a third
// step (speech-to-text model choice). The middleware only sends us here
// while `alias` is still `null` on this device. We keep the entered alias
// local until the final step commits (a chosen/skipped chat model,
// followed by a chosen/skipped STT model); only then do we persist the
// alias and route to /workspace/[instance]. An exit between steps
// therefore leaves the vault in "still onboarding" state and the wizard
// re-opens next launch.
const route = useRoute()
const { t } = useI18n()
const { errString } = useErrorString()
const { currentDeviceInfoAsync, updateDeviceAliasAsync } = useDevice()
const { recommendTiersAsync } = useCatalog()
const { downloadFromCatalogAsync } = useModels()
const { recommendTiersAsync: recommendSttTiersAsync } = useSttCatalog()
const { downloadFromCatalogAsync: downloadSttFromCatalogAsync } = useSttModels()
const { setPrefAsync } = usePreferences()
const { activateLocalModelAsync } = useChat()

const instanceName = computed(() => {
  const raw = route.params.instance
  return typeof raw === 'string'
    ? raw
    : Array.isArray(raw)
      ? (raw[0] ?? '')
      : ''
})

const step = ref<'alias' | 'model' | 'sttModel'>('alias')
const deviceInfo = ref<DeviceInfo | null>(null)
const alias = ref('')
const tiers = ref<TierRecommendation[]>([])
const downloadingId = ref<string | null>(null)
const downloadError = ref<string | null>(null)
const sttTiers = ref<TierRecommendation<SttCatalogEntry>[]>([])
const sttDownloadingId = ref<string | null>(null)
const sttDownloadError = ref<string | null>(null)
const loadError = ref<string | null>(null)

onMounted(async () => {
  try {
    deviceInfo.value = await currentDeviceInfoAsync()
    alias.value =
      deviceInfo.value?.alias ??
      deviceInfo.value?.hostname ??
      t('onboarding.alias.defaultPlaceholder')
  } catch (e) {
    loadError.value = errString(e)
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
    loadError.value = errString(e)
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
    await finishOnboardingAsync()
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

      <p v-if="loadError" class="text-sm text-destructive" role="alert">
        {{ t('errors.deviceInfoFailed') }}: {{ loadError }}
      </p>

      <div
        v-if="!deviceInfo && !loadError"
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
          v-else
          :tiers="sttTiers"
          :downloading-id="sttDownloadingId"
          :download-error="sttDownloadError"
          @choose="completeSttWithModel"
          @skip="finishOnboardingAsync"
          @back="step = 'model'"
        />
      </template>
    </div>
  </main>
</template>
