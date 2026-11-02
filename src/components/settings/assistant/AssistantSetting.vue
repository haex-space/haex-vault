<script setup lang="ts">
/**
 * "Sprachassistent" under KI → Sprache (spec 066 T045): until the wake word (PR D), the preview
 * switch `assistant.preview` makes conversations reachable through the indicator and the talk
 * shortcut. Below it, what the assistant still needs on this device (`assistant_readiness`); a
 * model haex can fetch links to its page, which asks before the download and shows its progress
 * (spec 043 FR-028). Saved on selection (spec 023 FR-021).
 */
import { invoke } from '@tauri-apps/api/core'
import {
  assistantLabelKey,
  parsePreview,
  PREVIEW_KEY,
} from '~/lib/assistant/settings'
import { detectPlatform } from '~/lib/wm/keybindings'
import { useSettingsDevice } from '~/components/settings/deviceContext'

type Missing =
  | 'sttModel'
  | 'vadModel'
  | 'wakeWordModel'
  | 'voice'
  | 'chatModel'
  | 'microphone'

const { t, locale } = useI18n()
const { errString } = useErrorString()
const { getPrefAsync } = usePreferences()
const assistant = useAssistantStore()
const device = useSettingsDevice()

const scope = computed(() => ({
  kind: 'device' as const,
  uuid: device.info.value.vaultDeviceUuid,
}))

const preview = ref(false)
const loading = ref(true)
const loadError = ref<string | null>(null)
const saving = ref(false)
const savedFlash = ref(false)
const opError = ref<string | null>(null)
/** What is missing; the wake word is left out, the preview does without it. */
const missing = ref<Missing[] | null>(null)

/** The page that fetches a missing piece. */
const PAGE: Partial<Record<Missing, string>> = {
  sttModel: '/ai/speech',
  voice: '/ai/speech/voice',
  chatModel: '/ai/local',
}

const keys = computed(() =>
  detectPlatform(
    navigator as Navigator & { userAgentData?: { platform?: string } },
  ) === 'mac'
    ? t('settings.assistantPage.keys.mac')
    : t('settings.assistantPage.keys.default'),
)

async function loadReadinessAsync() {
  try {
    const readiness = await invoke<{ missing: Missing[] }>(
      'assistant_readiness',
      { language: locale.value },
    )
    missing.value = readiness.missing.filter((m) => m !== 'wakeWordModel')
  } catch {
    // Builds without `voice` have no assistant to get ready.
    missing.value = null
  }
}

async function reloadAsync(quiet = false) {
  if (!quiet) loading.value = true
  loadError.value = null
  try {
    preview.value = parsePreview(await getPrefAsync(scope.value, PREVIEW_KEY))
    await loadReadinessAsync()
  } catch (e) {
    loadError.value = errString(e)
  } finally {
    loading.value = false
  }
}

async function toggleAsync(next: boolean) {
  preview.value = next
  saving.value = true
  savedFlash.value = false
  opError.value = null
  try {
    await assistant.setPreviewAsync(next)
    savedFlash.value = true
  } catch (e) {
    opError.value = t('settings.assistantPage.saveFailed', {
      reason: errString(e),
    })
  } finally {
    saving.value = false
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
          :title="t(assistantLabelKey('assistant.preview'))"
          :description="t('settings.assistantPage.previewHint', { keys })"
          label-for="settings-assistant-preview"
        >
          <ShadcnSwitch
            id="settings-assistant-preview"
            :model-value="preview"
            :disabled="saving"
            data-testid="settings-assistant-preview"
            @update:model-value="toggleAsync($event === true)"
          />
        </SettingsRow>
      </SettingsGroup>

      <SettingsGroup
        v-if="missing !== null"
        :label="t('settings.assistantPage.readiness')"
      >
        <SettingsRow
          v-if="missing.length === 0"
          :title="t('settings.assistantPage.ready')"
          data-testid="settings-assistant-ready"
        />
        <SettingsRow
          v-for="item in missing"
          :key="item"
          :title="t(`settings.assistantPage.missing.${item}`)"
          :to="PAGE[item]"
          :data-testid="`settings-assistant-missing-${item}`"
        />
      </SettingsGroup>

      <p v-if="savedFlash" class="px-1 text-xs text-success" role="status">
        {{ t('settings.assistantPage.saved') }}
      </p>
      <p v-if="opError" class="px-1 text-xs text-destructive" role="alert">
        {{ opError }}
      </p>
    </template>
  </section>
</template>
