<script setup lang="ts">
/**
 * "KI → Browser" (spec 065, T040): whether this device has an agent browser, which browser it
 * starts and whether it runs, the browser rights of the built-in agent, and removing the browser's
 * profile. Changes apply on selection (023 FR-021); each runs a guardrail action
 * (`settings.agentBrowser.*`), which no agent can call. The rights of external agents are on their
 * own pages under "Externe Agenten".
 */
import { invoke } from '@tauri-apps/api/core'
import type { AgentBrowserStatus } from '~/types/bindings/AgentBrowserStatus'
import type { BrowserRights } from '~/types/bindings/BrowserRights'

const { t } = useI18n()
const { errString } = useErrorString()
const setBuiltin = useActionOrThrow('settings.agentBrowser.builtin.set')
const setPath = useActionOrThrow('settings.agentBrowser.path.set')
const clearProfile = useActionOrThrow('settings.agentBrowser.profile.clear')

const status = ref<AgentBrowserStatus | null>(null)
const pathDraft = ref('')
const failure = ref<string | null>(null)
const confirmClear = ref(false)
const cleared = ref(false)

async function reloadAsync() {
  try {
    status.value = await invoke<AgentBrowserStatus>('agent_browser_status')
    pathDraft.value = status.value.chosenPath ?? ''
  } catch (error) {
    failure.value = errString(error)
  }
}

async function runAsync(action: () => Promise<unknown>) {
  failure.value = null
  try {
    await action()
  } catch (error) {
    failure.value = errString(error)
  }
  await reloadAsync()
}

function onBuiltin(rights: BrowserRights | null) {
  void runAsync(() => setBuiltin({ rights }))
}

function savePath() {
  const path = pathDraft.value.trim()
  if (path === (status.value?.chosenPath ?? '')) return
  void runAsync(() => setPath({ path: path || null }))
}

function resetPath() {
  pathDraft.value = ''
  void runAsync(() => setPath({ path: null }))
}

async function clearAsync() {
  confirmClear.value = false
  await runAsync(() => clearProfile({}))
  cleared.value = failure.value === null
}

onMounted(reloadAsync)
</script>

<template>
  <section class="flex flex-col gap-3" data-testid="agent-browser-settings">
    <SettingsNotOnThisDevice v-if="status && !status.available" />
    <template v-else-if="status">
      <SettingsGroup :label="t('agentBrowser.settings.browser')">
        <SettingsRow
          :title="t('agentBrowser.settings.state')"
          :description="
            status.running
              ? t('agentBrowser.settings.running', { product: status.running })
              : t('agentBrowser.settings.notRunning')
          "
          data-testid="agent-browser-state"
        />
        <SettingsRow
          :title="t('agentBrowser.settings.found')"
          :description="
            status.foundPath ?? t('agentBrowser.settings.noneFound')
          "
          data-testid="agent-browser-found"
        />
        <div class="flex flex-col gap-2 p-4">
          <label class="text-sm font-medium" for="agent-browser-path">
            {{ t('agentBrowser.settings.path') }}
          </label>
          <div class="flex gap-2">
            <UiInput
              id="agent-browser-path"
              v-model="pathDraft"
              class="min-w-0 flex-1"
              :placeholder="t('agentBrowser.settings.pathPlaceholder')"
              data-testid="agent-browser-path"
              @blur="savePath"
              @keydown.enter.prevent="savePath"
            />
            <UiButton
              v-if="status.chosenPath"
              type="button"
              variant="outline"
              @click="resetPath"
            >
              {{ t('agentBrowser.settings.pathReset') }}
            </UiButton>
          </div>
          <p class="text-xs text-muted-foreground">
            {{ t('agentBrowser.settings.pathHint') }}
          </p>
        </div>
      </SettingsGroup>

      <p
        v-if="status.profileProblem"
        class="rounded-xl bg-muted px-3 py-2 text-sm"
        role="status"
      >
        {{ t('agentBrowser.settings.profileProblem') }}
      </p>

      <p class="px-1 text-sm font-medium">
        {{ t('agentBrowser.settings.builtin') }}
      </p>
      <AgentsBrowserRightsFields
        :model-value="status.builtin"
        @update:model-value="onBuiltin"
      />

      <SettingsGroup :label="t('agentBrowser.settings.profile')">
        <div class="flex flex-wrap items-center justify-between gap-2 p-4">
          <span class="text-sm text-muted-foreground">
            {{
              cleared
                ? t('agentBrowser.settings.cleared')
                : t('agentBrowser.settings.profileHint')
            }}
          </span>
          <div class="flex gap-2">
            <template v-if="confirmClear">
              <UiButton
                type="button"
                variant="outline"
                @click="confirmClear = false"
              >
                {{ t('agentBrowser.settings.cancel') }}
              </UiButton>
              <UiButton
                type="button"
                variant="destructive"
                data-testid="agent-browser-clear-confirm"
                @click="clearAsync"
              >
                {{ t('agentBrowser.settings.clear') }}
              </UiButton>
            </template>
            <UiButton
              v-else
              type="button"
              variant="outline"
              data-testid="agent-browser-clear"
              @click="confirmClear = true"
            >
              {{ t('agentBrowser.settings.clear') }}
            </UiButton>
          </div>
        </div>
      </SettingsGroup>
    </template>
    <p v-if="failure" class="px-1 text-sm text-destructive" role="alert">
      {{ failure }}
    </p>
  </section>
</template>
