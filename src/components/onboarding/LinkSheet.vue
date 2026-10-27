<script setup lang="ts">
/**
 * "Mit einer Vault verknüpfen" on the landing page (spec 024, user story 5, FR-023 to FR-025): the
 * new installation enters the code a main device shows, gives this device a name and the new vault
 * its own passphrase. The vault is created here and stays only when the link finishes; nothing is
 * created before the user chose to link, so there is no vault to leave behind (FR-025). The
 * passphrase never leaves this device. A main device that uses its own Nostr-Relays or iroh-Relays
 * cannot tell a new installation about them, so the form takes them: the built-in ones are listed
 * and can be switched off, added ones switched off or removed, as in the settings.
 */
import {
  serverEntries,
  withEnabled,
  withoutServer,
} from '~/lib/sync/serverEntries'
import type { Ref } from 'vue'
import type { LinkJoinState } from '@bindings/LinkJoinState'

interface Props {
  open: boolean
}

const props = defineProps<Props>()
const emit = defineEmits<{
  'update:open': [value: boolean]
  /** The link finished; the vault exists and can be unlocked. */
  linked: [name: string]
}>()

const { t } = useI18n()
const fieldLabels = useFieldLabels()
const { joinStartAsync, joinCancelAsync, onJoinState } = useDeviceLink()
const { syncServersDefaultsAsync } = useSync()

const code = ref('')
const vaultName = ref('')
const deviceName = ref('')
const passphrase = ref('')
const passphraseConfirm = ref('')
/** The servers added here, and the ones of both kinds, built-in or added, switched off. */
const nostrRelays = ref<string[]>([])
const irohRelays = ref<string[]>([])
const disabledRelays = ref<string[]>([])
const defaultNostrRelays = ref<string[]>([])
const defaultIrohRelays = ref<string[]>([])
const submitting = ref(false)
const error = ref<string | null>(null)
/** `null` while the form is shown. */
const state = ref<LinkJoinState | null>(null)

const running = computed(
  () =>
    state.value !== null &&
    state.value.state !== 'done' &&
    state.value.state !== 'failed',
)

// Keep template expressions narrowed when vue-tsc checks the ref across branches.
const stateKind = computed(() => state.value?.state ?? '')
const doneVaultName = computed(() => {
  const current = state.value
  return current?.state === 'done' ? current.vaultName : ''
})
const failureReason = computed(() => {
  const current = state.value
  return current?.state === 'failed' ? current.reason : 'openFailed'
})

function reset() {
  code.value = ''
  vaultName.value = ''
  deviceName.value = ''
  passphrase.value = ''
  passphraseConfirm.value = ''
  nostrRelays.value = []
  irohRelays.value = []
  disabledRelays.value = []
  error.value = null
  state.value = null
}

let stopListening: (() => void) | undefined
onMounted(async () => {
  // Only for showing; linking works without, with the backend's own built-in servers.
  syncServersDefaultsAsync()
    .then((defaults) => {
      defaultNostrRelays.value = defaults.nostrRelays
      defaultIrohRelays.value = defaults.irohRelays
    })
    .catch(() => {})
  stopListening = await onJoinState((next) => {
    if (state.value !== null) state.value = next
  })
})
onBeforeUnmount(() => stopListening?.())

// Closing the sheet ends a link in progress; its vault is removed (FR-025).
let closeRequested = false
let cancelOnClose: Promise<void> | undefined

watch(
  () => props.open,
  (isOpen) => {
    closeRequested = !isOpen
    if (isOpen) return
    if (running.value) {
      cancelOnClose = joinCancelAsync().finally(() => {
        cancelOnClose = undefined
        if (!props.open) reset()
      })
      return
    }
    reset()
  },
)

const canSubmit = computed(
  () =>
    code.value.trim().length > 0 &&
    vaultName.value.length > 0 &&
    deviceName.value.trim().length > 0 &&
    passphrase.value.length >= 8 &&
    passphrase.value === passphraseConfirm.value &&
    !submitting.value,
)

async function onSubmit() {
  if (!canSubmit.value) return
  submitting.value = true
  error.value = null
  // Shown from the first moment, so an event that arrives before `joinStartAsync` returns is not lost.
  state.value = { state: 'searching' }
  try {
    const started = await joinStartAsync({
      code: code.value,
      vaultName: vaultName.value,
      deviceName: deviceName.value.trim(),
      passphrase: passphrase.value,
      servers:
        nostrRelays.value.length ||
        irohRelays.value.length ||
        disabledRelays.value.length
          ? {
              nostrRelays: nostrRelays.value,
              irohRelays: irohRelays.value,
              disabled: disabledRelays.value,
            }
          : undefined,
    })
    if (closeRequested) {
      await cancelOnClose
      await joinCancelAsync()
      return
    }
    // An event may already have moved on; only the first state is replaced.
    if (state.value?.state === 'searching') state.value = started
    // The passphrase is only needed to create the vault; do not keep it in the form.
    passphrase.value = ''
    passphraseConfirm.value = ''
  } catch (e: unknown) {
    state.value = null
    // The backend names error kinds in PascalCase; the texts are keyed in camelCase.
    const { kind, reason } = (e as { kind?: string; reason?: string }) ?? {}
    const key = kind
      ? kind.charAt(0).toLowerCase() + kind.slice(1)
      : 'openFailed'
    // An invalid server says which one; every other kind has its own text.
    error.value =
      kind === 'InvalidInput' && reason
        ? reason
        : t(`errors.${key}`, t('errors.openFailed'))
  } finally {
    submitting.value = false
  }
}

async function onCancel() {
  await joinCancelAsync()
}

type Kind = 'nostr' | 'iroh'
const added: Record<Kind, Ref<string[]>> = {
  nostr: nostrRelays,
  iroh: irohRelays,
}

async function addRelayAsync(kind: Kind, url: string): Promise<boolean> {
  added[kind].value = [...added[kind].value, url]
  return true
}

function onToggleRelay(url: string, enabled: boolean) {
  disabledRelays.value = withEnabled(disabledRelays.value, url, enabled)
}

function onRemoveRelay(kind: Kind, url: string) {
  const rest = withoutServer(added[kind].value, disabledRelays.value, url)
  added[kind].value = rest.added
  disabledRelays.value = rest.disabled
}

/** Back to the form after a failure. */
function onRetry() {
  state.value = null
  error.value = null
}

function onOpenVault() {
  if (state.value?.state !== 'done') return
  const name = state.value.vaultName
  emit('update:open', false)
  emit('linked', name)
}
</script>

<template>
  <UiDrawerModal
    :open="props.open"
    :title="t('onboarding.link.title')"
    @update:open="$emit('update:open', $event)"
  >
    <template #content>
      <form
        v-if="state === null"
        id="link-form"
        class="space-y-4 px-6 py-2"
        @submit.prevent="onSubmit"
      >
        <p class="text-sm text-muted-foreground">
          {{ t('onboarding.link.intro') }}
        </p>
        <UiInput
          id="link-code"
          v-model="code"
          :label="t('onboarding.link.code')"
          :labels="fieldLabels.input.value"
          autofocus
          autocomplete="off"
          spellcheck="false"
          class="font-mono uppercase"
          :placeholder="t('onboarding.link.codePlaceholder')"
          data-testid="link-code"
        />
        <UiInput
          id="link-device-name"
          v-model="deviceName"
          :label="t('onboarding.link.deviceName')"
          :labels="fieldLabels.input.value"
          data-testid="link-device-name"
        />
        <UiInput
          id="link-vault-name"
          v-model="vaultName"
          :label="t('onboarding.link.vaultName')"
          :labels="fieldLabels.input.value"
          data-testid="link-vault-name"
        />
        <div class="space-y-1.5">
          <UiInputPassword
            id="link-passphrase"
            v-model="passphrase"
            :label="t('onboarding.link.passphrase')"
            :labels="fieldLabels.password.value"
          />
          <p class="text-xs text-muted-foreground">
            {{ t('onboarding.link.passphraseHint') }}
          </p>
        </div>
        <UiInputPassword
          id="link-passphrase-confirm"
          v-model="passphraseConfirm"
          :label="t('onboarding.create.passphraseConfirm')"
          :labels="fieldLabels.password.value"
        />
        <SettingsServerList
          :label="t('onboarding.link.nostr')"
          :description="t('onboarding.link.nostrDescription')"
          :entries="
            serverEntries(defaultNostrRelays, nostrRelays, disabledRelays)
          "
          placeholder="wss://"
          :none-note="t('settings.federation.servers.nostrNone')"
          test-id="link-servers-nostr"
          :add-async="(url: string) => addRelayAsync('nostr', url)"
          @toggle="onToggleRelay"
          @remove="onRemoveRelay('nostr', $event)"
        />
        <SettingsServerList
          :label="t('onboarding.link.iroh')"
          :description="t('onboarding.link.irohDescription')"
          :entries="
            serverEntries(defaultIrohRelays, irohRelays, disabledRelays)
          "
          placeholder="https://"
          :none-note="t('settings.federation.servers.irohNone')"
          test-id="link-servers-iroh"
          :add-async="(url: string) => addRelayAsync('iroh', url)"
          @toggle="onToggleRelay"
          @remove="onRemoveRelay('iroh', $event)"
        />
        <p v-if="error" class="text-sm text-destructive" role="alert">
          {{ error }}
        </p>
      </form>

      <div
        v-else
        class="flex flex-col items-center gap-3 px-6 py-6 text-center"
        role="status"
        data-testid="link-progress"
        :data-state="stateKind"
      >
        <template v-if="running">
          <Icon name="lucide:loader-circle" class="size-8 animate-spin" />
          <p class="font-medium">
            {{ t(`onboarding.link.state.${stateKind}`) }}
          </p>
          <p
            v-if="stateKind === 'searching'"
            class="text-sm text-muted-foreground"
          >
            {{ t('onboarding.link.searchingHint') }}
          </p>
        </template>
        <template v-else-if="stateKind === 'done'">
          <Icon name="lucide:circle-check" class="size-8 text-primary" />
          <p class="font-medium">
            {{ t('onboarding.link.done', { name: doneVaultName }) }}
          </p>
          <p class="text-sm text-muted-foreground">
            {{ t('onboarding.link.doneHint') }}
          </p>
        </template>
        <template v-else-if="stateKind === 'failed'">
          <Icon name="lucide:circle-alert" class="size-8 text-destructive" />
          <p class="font-medium" role="alert">
            {{ t(`onboarding.link.failed.${failureReason}`) }}
          </p>
          <p class="text-sm text-muted-foreground">
            {{ t('onboarding.link.failedHint') }}
          </p>
        </template>
      </div>
    </template>
    <template #footer>
      <UiButton
        v-if="state === null"
        form="link-form"
        type="submit"
        :disabled="!canSubmit"
        :loading="submitting"
        class="w-full"
        data-testid="link-submit"
      >
        {{ t('onboarding.link.submit') }}
      </UiButton>
      <UiButton
        v-else-if="running"
        variant="outline"
        class="w-full"
        @click="onCancel"
      >
        {{ t('onboarding.link.cancel') }}
      </UiButton>
      <UiButton
        v-else-if="state.state === 'done'"
        class="w-full"
        data-testid="link-open-vault"
        @click="onOpenVault"
      >
        {{ t('onboarding.link.openVault') }}
      </UiButton>
      <UiButton v-else class="w-full" @click="onRetry">
        {{ t('onboarding.link.retry') }}
      </UiButton>
    </template>
  </UiDrawerModal>
</template>
