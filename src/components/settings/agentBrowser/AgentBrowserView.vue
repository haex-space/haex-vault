<script setup lang="ts">
/**
 * "KI → Browser" (spec 065, T040): whether this device has an agent browser, which browser it
 * starts and whether it runs, the trackers, the cookies and removing the browser's profile.
 * Changes apply on selection (023 FR-021); each runs a guardrail action
 * (`settings.agentBrowser.*`), which no agent can call. What an agent may do in the browser is in
 * its agent profile (spec 083, "KI → Agentenprofile").
 */
import { invoke } from '@tauri-apps/api/core'
import { toast } from 'vue-sonner'
import type { AgentBrowserStatus } from '~/types/bindings/AgentBrowserStatus'

const { t } = useI18n()
const { errString } = useErrorString()
const setPath = useActionOrThrow('settings.agentBrowser.path.set')
const clearProfile = useActionOrThrow('settings.agentBrowser.profile.clear')

const status = ref<AgentBrowserStatus | null>(null)
const pathDraft = ref('')
const confirmClear = ref(false)
const cleared = ref(false)

/** Reads the status again; `false` when that failed (shown as a toast). */
async function reloadAsync(): Promise<boolean> {
  try {
    status.value = await invoke<AgentBrowserStatus>('agent_browser_status')
    pathDraft.value = status.value.chosenPath ?? ''
    return true
  } catch (error) {
    toast.error(errString(error))
    return false
  }
}

/** Runs one action and reloads; `false` when either failed (shown as a toast). */
async function runAsync(action: () => Promise<unknown>): Promise<boolean> {
  let ok = true
  try {
    await action()
  } catch (error) {
    toast.error(errString(error))
    ok = false
  }
  return (await reloadAsync()) && ok
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
  cleared.value = await runAsync(() => clearProfile({}))
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

      <SettingsAgentBrowserTrackerLists />

      <SettingsAgentBrowserDnsSetting />

      <SettingsAgentBrowserProfileCookies :running="status.running != null" />

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
  </section>
</template>
