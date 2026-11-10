<script setup lang="ts">
import type { UnlistenFn } from '@tauri-apps/api/event'
import { openUrl } from '@tauri-apps/plugin-opener'
import { toast } from 'vue-sonner'
import type { DelegateVendor, Provider } from '~/composables/useProviders'

const { t } = useI18n()
const { capabilities } = useDeviceCapabilities()
const { errString } = useErrorString()
const fieldLabels = useFieldLabels()
const { listAsync, onDelegateConnectProgress } = useProviders()
// Spec 020 FR-024: provider writes run catalog actions (connect/submit are guardrails).
const refreshModels = useActionOrThrow('settings.delegate.refreshModels')
const connect = useActionOrThrow('settings.delegate.connectProvider')
const submitCode = useActionOrThrow('settings.delegate.submitCode')

const VENDORS: DelegateVendor[] = ['claude', 'codex']

const providerList = ref<Provider[]>([])
const loading = ref(true)

const connecting = reactive<Record<DelegateVendor, boolean>>({
  claude: false,
  codex: false,
})
const awaitingCode = reactive<Record<DelegateVendor, boolean>>({
  claude: false,
  codex: false,
})
const progressUrl = reactive<Record<DelegateVendor, string | null>>({
  claude: null,
  codex: null,
})
const progressCode = reactive<Record<DelegateVendor, string | null>>({
  claude: null,
  codex: null,
})
const codeInput = reactive<Record<DelegateVendor, string>>({
  claude: '',
  codex: '',
})
// "Refresh models" (spec 012 FR-022): re-fetches a connected provider's
// models and their capabilities without a new sign-in.
const refreshing = reactive<Record<DelegateVendor, boolean>>({
  claude: false,
  codex: false,
})

let unlisten: UnlistenFn | null = null

/** A connect step failed; shown as a toast. */
function connectFailed(reason: string) {
  toast.error(`${t('settings.cliDelegate.error')}: ${reason}`)
}

function vendorLabel(vendor: DelegateVendor): string {
  return t(`chat.model.delegate.${vendor}`)
}

function connectedProvider(vendor: DelegateVendor): Provider | undefined {
  return providerList.value.find(
    (p) => p.kind === 'cli_delegate' && p.adapter === vendor,
  )
}

/** A `quiet` reload (after a change from elsewhere) keeps the shown providers until the new ones are read. */
async function reloadAsync(quiet = false) {
  if (!quiet) loading.value = true
  try {
    providerList.value = await listAsync()
  } catch (e) {
    toast.error(`${t('errors.prefLoadFailed')}: ${errString(e)}`)
  } finally {
    loading.value = false
  }
}

function resetVendorState(vendor: DelegateVendor) {
  awaitingCode[vendor] = false
  progressUrl[vendor] = null
  progressCode[vendor] = null
  codeInput[vendor] = ''
}

/**
 * Re-fetches the connected provider's models. The chat page re-reads its
 * model lists on mount, so this needs no store coupling; on failure the
 * previously stored capabilities stay untouched (the backend keeps the old
 * cache when a refresh fails).
 */
async function onRefreshModels(vendor: DelegateVendor) {
  const provider = connectedProvider(vendor)
  if (!provider) return
  refreshing[vendor] = true
  try {
    await refreshModels({ providerId: provider.id })
  } catch (e) {
    toast.error(`${t('settings.cliDelegate.refreshFailed')}: ${errString(e)}`)
  } finally {
    refreshing[vendor] = false
  }
}

async function onConnect(vendor: DelegateVendor) {
  resetVendorState(vendor)
  connecting[vendor] = true
  try {
    const result = (await connect({ vendor, name: vendorLabel(vendor) })) as {
      status: string
    }
    if (result.status === 'awaiting_code') {
      awaitingCode[vendor] = true
    } else {
      await reloadAsync()
      connecting[vendor] = false
    }
  } catch (e) {
    connectFailed(errString(e))
    connecting[vendor] = false
  }
}

async function onSubmitCode(vendor: DelegateVendor) {
  if (!codeInput[vendor].trim()) return
  try {
    await submitCode({ code: codeInput[vendor], name: vendorLabel(vendor) })
    await reloadAsync()
    awaitingCode[vendor] = false
    connecting[vendor] = false
  } catch (e) {
    // The backend keeps the flow open on failure (a rejected code commonly
    // re-prompts rather than exiting) — stay in the code-entry state so the
    // operator can retry without restarting the whole connect flow.
    connectFailed(errString(e))
  }
}

onMounted(async () => {
  await reloadAsync()
  unlisten = await onDelegateConnectProgress(async (event) => {
    if (
      event.status === 'awaiting_browser' ||
      event.status === 'awaiting_code'
    ) {
      progressUrl[event.vendor] = event.url ?? null
      progressCode[event.vendor] = event.code ?? null
    } else if (event.status === 'success') {
      // Claude can complete on its own once the browser round-trip
      // finishes, without any submitCliDelegateCodeAsync call ever
      // resolving — this event is the only signal for that case.
      awaitingCode[event.vendor] = false
      connecting[event.vendor] = false
      await reloadAsync()
    } else if (event.status === 'error') {
      awaitingCode[event.vendor] = false
      if (event.message) connectFailed(event.message)
      connecting[event.vendor] = false
    }
  })
})

onVaultTablesChanged(['providers'], () => reloadAsync(true))

onBeforeUnmount(() => {
  unlisten?.()
})

/** Whether the row shows anything under its name: a running connect. The new state is the
 * confirmation of a connect or a refresh; there is no success line. */
function hasDetails(vendor: DelegateVendor): boolean {
  return Boolean(awaitingCode[vendor] || connecting[vendor])
}
</script>

<template>
  <section class="flex flex-col gap-3">
    <SettingsNotOnThisDevice v-if="capabilities?.cliDelegates === false" />
    <div v-if="loading" class="text-sm text-muted-foreground">
      {{ t('onboarding.wizard.loadingDeviceInfo') }}
    </div>

    <SettingsGroup v-if="!loading">
      <SettingsRow
        v-for="vendor in VENDORS"
        :key="vendor"
        :title="vendorLabel(vendor)"
      >
        <template
          v-if="connectedProvider(vendor) && !awaitingCode[vendor]"
          #description
        >
          <span class="text-success">{{
            t('settings.cliDelegate.connected')
          }}</span>
        </template>
        <template v-if="!awaitingCode[vendor] && !connecting[vendor]">
          <UiButton
            type="button"
            variant="outline"
            :disabled="capabilities?.cliDelegates === false"
            @click="onConnect(vendor)"
          >
            {{
              connectedProvider(vendor)
                ? t('settings.cliDelegate.reconnect')
                : t('settings.cliDelegate.connect')
            }}
          </UiButton>
          <UiButton
            v-if="connectedProvider(vendor)"
            type="button"
            variant="outline"
            :disabled="refreshing[vendor]"
            @click="onRefreshModels(vendor)"
          >
            {{
              refreshing[vendor]
                ? t('settings.cliDelegate.refreshing')
                : t('settings.cliDelegate.refreshModels')
            }}
          </UiButton>
        </template>
        <template v-if="hasDetails(vendor)" #below>
          <div class="flex flex-col gap-2">
            <div v-if="awaitingCode[vendor]" class="flex flex-col gap-2">
              <p class="text-sm text-muted-foreground">
                {{ t('settings.cliDelegate.awaitingAuto') }}
              </p>
              <p class="text-sm">
                {{ t('settings.cliDelegate.openUrlPrompt') }}
              </p>
              <a
                v-if="progressUrl[vendor]"
                :href="progressUrl[vendor]!"
                class="text-sm underline text-primary hover:text-primary/80 break-all"
                @click.prevent="openUrl(progressUrl[vendor]!)"
              >
                {{ progressUrl[vendor] }}
              </a>
              <UiInput
                v-model="codeInput[vendor]"
                :label="t('settings.cliDelegate.codeInputLabel')"
                :labels="fieldLabels.input.value"
                label-bg="var(--muted)"
                type="text"
                autocomplete="off"
                @keyup.enter="onSubmitCode(vendor)"
              />
              <div class="flex items-center gap-3 flex-wrap">
                <UiButton
                  type="button"
                  :disabled="!codeInput[vendor].trim()"
                  @click="onSubmitCode(vendor)"
                >
                  {{ t('settings.cliDelegate.submitCode') }}
                </UiButton>
              </div>
            </div>
            <div v-else-if="connecting[vendor]" class="flex flex-col gap-2">
              <p class="text-sm text-muted-foreground">
                {{ t('settings.cliDelegate.connecting') }}
              </p>
              <template v-if="progressUrl[vendor]">
                <a
                  :href="progressUrl[vendor]!"
                  class="text-sm underline text-primary hover:text-primary/80 break-all"
                  @click.prevent="openUrl(progressUrl[vendor]!)"
                >
                  {{ progressUrl[vendor] }}
                </a>
                <p v-if="progressCode[vendor]" class="text-sm">
                  {{ t('settings.cliDelegate.enterCodePrompt') }}
                  <strong>{{ progressCode[vendor] }}</strong>
                </p>
              </template>
            </div>
          </div>
        </template>
      </SettingsRow>
    </SettingsGroup>
  </section>
</template>
