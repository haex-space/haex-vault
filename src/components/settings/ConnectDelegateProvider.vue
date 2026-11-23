<script setup lang="ts">
import type { UnlistenFn } from '@tauri-apps/api/event'
import { toast } from 'vue-sonner'
import type { DelegateVendor, Provider } from '~/composables/useProviders'

const { t } = useI18n()
const { capabilities } = useDeviceCapabilities()
const { errString } = useErrorString()
const { listAsync, onDelegateConnectProgress, cancelConnectCliDelegateAsync } =
  useProviders()
// Spec 020 FR-024: provider writes run catalog actions (connect/submit are guardrails).
const cliTools = useCliTools()
const refreshModels = useActionOrThrow('settings.delegate.refreshModels')
const connect = useActionOrThrow('settings.delegate.connectProvider')
const submitCode = useActionOrThrow('settings.delegate.submitCode')
const removeDelegate = useActionOrThrow('settings.delegate.remove')

const VENDORS: DelegateVendor[] = ['claude', 'codex', 'abacusai']

const providerList = ref<Provider[]>([])
const loading = ref(true)

const connecting = reactive<Record<DelegateVendor, boolean>>({
  claude: false,
  codex: false,
  abacusai: false,
})
// A submitted code is being exchanged: the button shows it, since the program may take a while.
const submitting = reactive<Record<DelegateVendor, boolean>>({
  claude: false,
  codex: false,
  abacusai: false,
})
const awaitingCode = reactive<Record<DelegateVendor, boolean>>({
  claude: false,
  codex: false,
  abacusai: false,
})
const progressUrl = reactive<Record<DelegateVendor, string | null>>({
  claude: null,
  codex: null,
  abacusai: null,
})
const progressCode = reactive<Record<DelegateVendor, string | null>>({
  claude: null,
  codex: null,
  abacusai: null,
})
const codeInput = reactive<Record<DelegateVendor, string>>({
  claude: '',
  codex: '',
  abacusai: '',
})
// "Refresh models" (spec 012 FR-022): re-fetches a connected provider's
// models and their capabilities without a new sign-in.
const refreshing = reactive<Record<DelegateVendor, boolean>>({
  claude: false,
  codex: false,
  abacusai: false,
})

// The sign-in of a vendor needs more room than its row has: it opens in a dialog of its own.
const signInOpen = reactive<Record<DelegateVendor, boolean>>({
  claude: false,
  codex: false,
  abacusai: false,
})
// A provider that signs in with the account's own credentials asks for the e-mail address first.
const emailOpen = reactive<Record<DelegateVendor, boolean>>({
  claude: false,
  codex: false,
  abacusai: false,
})
const emailInput = reactive<Record<DelegateVendor, string>>({
  claude: '',
  codex: '',
  abacusai: '',
})
const removing = reactive<Record<DelegateVendor, boolean>>({
  claude: false,
  codex: false,
  abacusai: false,
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
  signInOpen[vendor] = false
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

/** The vendors whose sign-in starts with the e-mail address of the account. */
function asksForEmail(vendor: DelegateVendor): boolean {
  return vendor === 'abacusai'
}

/** The backend's code for a sign-in the person ended by closing the dialog. */
function isSigninCancelled(e: unknown): boolean {
  return (
    typeof e === 'object' &&
    e !== null &&
    'reason' in e &&
    typeof e.reason === 'string' &&
    e.reason.startsWith('delegate.signin_cancelled')
  )
}

async function onConnect(vendor: DelegateVendor) {
  resetVendorState(vendor)
  if (asksForEmail(vendor)) {
    emailOpen[vendor] = true
    return
  }
  connecting[vendor] = true
  try {
    const result = (await connect({ vendor, name: vendorLabel(vendor) })) as {
      status: string
    }
    if (result.status === 'awaiting_code') {
      awaitingCode[vendor] = true
      signInOpen[vendor] = true
    } else {
      await reloadAsync()
      connecting[vendor] = false
    }
  } catch (e) {
    connectFailed(errString(e))
    connecting[vendor] = false
  }
}

async function onSubmitEmail(vendor: DelegateVendor) {
  const email = emailInput[vendor].trim()
  if (!email || connecting[vendor]) return
  emailOpen[vendor] = false
  connecting[vendor] = true
  try {
    await connect({ vendor, name: vendorLabel(vendor), email })
    await reloadAsync()
  } catch (e) {
    // Closing the sign-in dialog ends the wait; that is the person's own choice, not a failure.
    if (!isSigninCancelled(e)) connectFailed(errString(e))
  } finally {
    resetVendorState(vendor)
    emailInput[vendor] = ''
    connecting[vendor] = false
  }
}

/** Closing the sign-in dialog while the service waits for the person ends that wait. */
watch(
  () => signInOpen.abacusai,
  async (open) => {
    if (open || !connecting.abacusai) return
    try {
      await cancelConnectCliDelegateAsync('abacusai')
    } catch {
      // Nothing waits any more: the flow has ended by itself.
    }
  },
)

/** The backend's code for a sign-in code the program did not accept (`providers/connect.rs`). */
const CODE_REJECTED = 'cli_delegate.code_rejected'

function isCodeRejected(e: unknown): boolean {
  return (
    typeof e === 'object' &&
    e !== null &&
    'reason' in e &&
    typeof e.reason === 'string' &&
    e.reason.startsWith(CODE_REJECTED)
  )
}

async function onSubmitCode(vendor: DelegateVendor) {
  if (!codeInput[vendor].trim() || submitting[vendor]) return
  submitting[vendor] = true
  try {
    await submitCode({ code: codeInput[vendor], name: vendorLabel(vendor) })
    await reloadAsync()
    awaitingCode[vendor] = false
    connecting[vendor] = false
    signInOpen[vendor] = false
  } catch (e) {
    // A failed submit ends the flow: after a code that was not accepted the program waits for a
    // new sign-in, so the next try starts with "Connect" and a new link.
    resetVendorState(vendor)
    connecting[vendor] = false
    if (isCodeRejected(e)) {
      toast.error(t('settings.cliDelegate.codeRejected'))
    } else {
      connectFailed(errString(e))
    }
  } finally {
    submitting[vendor] = false
  }
}

onMounted(async () => {
  await reloadAsync()
  // The tools of this vault: a connect needs the vault's own installation (spec 094 FR-004).
  void cliTools.startAsync()
  unlisten = await onDelegateConnectProgress(async (event) => {
    if (
      event.status === 'awaiting_browser' ||
      event.status === 'awaiting_code'
    ) {
      progressUrl[event.vendor] = event.url ?? null
      progressCode[event.vendor] = event.code ?? null
      signInOpen[event.vendor] = true
    } else if (event.status === 'success') {
      // Claude can complete on its own once the browser round-trip
      // finishes, without any submitCliDelegateCodeAsync call ever
      // resolving — this event is the only signal for that case.
      awaitingCode[event.vendor] = false
      signInOpen[event.vendor] = false
      if (!asksForEmail(event.vendor)) {
        connecting[event.vendor] = false
        await reloadAsync()
      }
    } else if (event.status === 'error') {
      awaitingCode[event.vendor] = false
      signInOpen[event.vendor] = false
      // The sign-in of an account vendor reports its failure with the rejected call itself.
      if (event.message && !asksForEmail(event.vendor)) {
        connectFailed(event.message)
        connecting[event.vendor] = false
      }
    }
  })
})

onVaultTablesChanged(['providers'], () => reloadAsync(true))

onBeforeUnmount(() => {
  unlisten?.()
  if (connecting.abacusai) {
    void cancelConnectCliDelegateAsync('abacusai').catch(() => {
      // The flow may already have ended while this view was closing.
    })
  }
  cliTools.stop()
})

/** Removes the vendor's program from this vault and cuts its connection. */
async function onRemove(vendor: DelegateVendor) {
  removing[vendor] = true
  try {
    await removeDelegate({ vendor })
    await reloadAsync(true)
    await cliTools.refreshAsync()
  } catch (e) {
    toast.error(errString(e), { id: `cli-tool-remove-${vendor}` })
    await Promise.all([reloadAsync(true), cliTools.refreshAsync()])
  } finally {
    removing[vendor] = false
  }
}
</script>

<template>
  <section class="flex flex-col gap-3">
    <SettingsNotOnThisDevice v-if="capabilities?.cliDelegates === false" />
    <div v-if="loading" class="text-sm text-muted-foreground">
      {{ t('onboarding.wizard.loadingDeviceInfo') }}
    </div>

    <SettingsGroup v-if="!loading">
      <template v-for="vendor in VENDORS" :key="vendor">
        <template v-if="cliTools.statusOf(vendor) || connectedProvider(vendor)">
          <SettingsDelegateRow
            :vendor="vendor"
            :title="vendorLabel(vendor)"
            :status="cliTools.statusOf(vendor)"
            :progress="cliTools.progress[vendor]"
            :connected="Boolean(connectedProvider(vendor))"
            :connecting="connecting[vendor]"
            :refreshing="refreshing[vendor]"
            :removing="removing[vendor]"
            :connect-blocked="capabilities?.cliDelegates === false"
            @install="cliTools.installAsync(vendor)"
            @revert="cliTools.revertAsync(vendor)"
            @cancel="cliTools.cancelAsync(vendor)"
            @connect="onConnect(vendor)"
            @show-sign-in="signInOpen[vendor] = true"
            @refresh="onRefreshModels(vendor)"
            @remove="onRemove(vendor)"
          />
          <SettingsDelegateEmailDialog
            v-if="asksForEmail(vendor)"
            v-model:open="emailOpen[vendor]"
            v-model:email="emailInput[vendor]"
            :title="
              t('settings.cliDelegate.signInTitle', {
                name: vendorLabel(vendor),
              })
            "
            :submitting="connecting[vendor]"
            @submit="onSubmitEmail(vendor)"
          />
          <SettingsDelegateSignInDialog
            v-model:open="signInOpen[vendor]"
            v-model:code="codeInput[vendor]"
            :title="
              t('settings.cliDelegate.signInTitle', {
                name: vendorLabel(vendor),
              })
            "
            :url="progressUrl[vendor]"
            :device-code="progressCode[vendor]"
            :awaiting-code="awaitingCode[vendor]"
            :submitting="submitting[vendor]"
            @submit="onSubmitCode(vendor)"
          />
        </template>
      </template>
    </SettingsGroup>
  </section>
</template>
