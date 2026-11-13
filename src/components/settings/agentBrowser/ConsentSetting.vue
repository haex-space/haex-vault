<script setup lang="ts">
/**
 * "Cookie-Banner" in "KI → Browser" (spec 065 FR-066): whether the agent browser declines the
 * optional cookies of cookie banners. It is on until the user turns it off, never accepts anything
 * and applies to the tabs the agent opens from then on. The change applies on selection (023
 * FR-021) and runs the guardrail action `settings.agentBrowser.consent.set`, which no agent can
 * call; a failure is a toast.
 */
import { invoke } from '@tauri-apps/api/core'
import { toast } from 'vue-sonner'
import type { ConsentSettings } from '~/types/bindings/ConsentSettings'

const { t } = useI18n()
const { errString } = useErrorString()
const setConsent = useActionOrThrow('settings.agentBrowser.consent.set')

const settings = ref<ConsentSettings | null>(null)

async function reloadAsync() {
  try {
    settings.value = await invoke<ConsentSettings>('agent_browser_consent')
  } catch (error) {
    toast.error(errString(error))
  }
}

onMounted(reloadAsync)

async function setRejectAsync(rejectOptional: boolean) {
  try {
    settings.value = (await setConsent({
      settings: { rejectOptional },
    })) as ConsentSettings
  } catch (error) {
    toast.error(errString(error))
    await reloadAsync()
  }
}
</script>

<template>
  <template v-if="settings">
    <p class="px-1 text-sm font-medium">
      {{ t('agentBrowser.settings.consent.title') }}
    </p>
    <SettingsGroup data-testid="agent-browser-consent">
      <SettingsRow
        :title="t('agentBrowser.settings.consent.reject')"
        :description="t('agentBrowser.settings.consent.rejectHint')"
        label-for="agent-browser-consent-switch"
      >
        <ShadcnSwitch
          id="agent-browser-consent-switch"
          :model-value="settings.rejectOptional"
          data-testid="agent-browser-consent-switch"
          @update:model-value="setRejectAsync($event === true)"
        />
      </SettingsRow>
    </SettingsGroup>
  </template>
</template>
