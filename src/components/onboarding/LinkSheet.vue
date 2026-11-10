<script setup lang="ts">
/**
 * "Mit einer Vault verknüpfen" on the landing page (spec 024, user story 5, FR-023 to FR-025): the
 * new installation enters the code a main device shows, gives this device a name and the new vault
 * its own passphrase. The vault is created here and stays only when the link finishes; nothing is
 * created before the user chose to link, so there is no vault to leave behind (FR-025). The
 * passphrase never leaves this device. A main device that uses its own Nostr-Relays or iroh-Relays
 * cannot tell a new installation about them, so the form takes them: the built-in ones are listed
 * and can be switched off, added ones switched off or removed, as in the settings.
 *
 * Spec 085: the main device's QR code (or the link it copies) carries the code, the vault's name
 * and those servers. A scan on a phone, a scan with the webcam, a paste or typing it fills the
 * form, and only the name of this device and the passphrase are left; the server blocks fold to
 * one line then. Nothing is started by a scan alone.
 */
import { toast } from 'vue-sonner'
import {
  serverEntries,
  withEnabled,
  withoutServer,
} from '~/lib/sync/serverEntries'
import {
  isLinkTextInput,
  serversTakenOver,
  withLinkText,
  type LinkFormFields,
} from '~/lib/sync/linkForm'
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
const { errString } = useErrorString()
const { joinStartAsync, joinCancelAsync, onJoinState, parseTextAsync } =
  useDeviceLink()
const { usesPreview, scanAsync } = useQrScan()
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
/** The servers came with a link text; the blocks fold to one line until the user opens them. */
const serversFromLink = ref(false)
const serversOpen = ref(false)
/** The camera preview of a desktop is showing. */
const scannerOpen = ref(false)
/** `null` while the form is shown. */
const state = ref<LinkJoinState | null>(null)
let parseRun = 0
let parsePending: Promise<boolean> | null = null

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

// The end of a link shows as a toast; the sheet keeps the button that goes on from there.
watch(stateKind, (kind) => {
  if (kind === 'failed')
    toast.error(t(`onboarding.link.failed.${failureReason.value}`), {
      description: t('onboarding.link.failedHint'),
    })
  else if (kind === 'done')
    toast.success(t('onboarding.link.done', { name: doneVaultName.value }), {
      description: t('onboarding.link.doneHint'),
    })
})

function reset() {
  parseRun += 1
  parsePending = null
  code.value = ''
  vaultName.value = ''
  deviceName.value = ''
  passphrase.value = ''
  passphraseConfirm.value = ''
  nostrRelays.value = []
  irohRelays.value = []
  disabledRelays.value = []
  serversFromLink.value = false
  serversOpen.value = false
  scannerOpen.value = false
  state.value = null
}

function fields(): LinkFormFields {
  return {
    code: code.value,
    vaultName: vaultName.value,
    nostrRelays: nostrRelays.value,
    irohRelays: irohRelays.value,
    disabledRelays: disabledRelays.value,
    serversFromLink: serversFromLink.value,
  }
}

/** The error of reading a text: its own words for the two kinds the backend names. */
function readError(e: unknown): string {
  const kind = (e as { kind?: string })?.kind
  if (kind === 'NotALinkCode') return t('onboarding.import.errors.notALinkCode')
  if (kind === 'UnsupportedVersion')
    return t('onboarding.import.errors.unsupportedVersion')
  return errString(e)
}

/**
 * Reads a link text or code the backend, and fills the form with it. Device name and passphrases
 * stay as they are; a text that is no link code fills nothing and says so in a toast.
 */
async function applyLinkTextAsync(raw: string): Promise<boolean> {
  const run = ++parseRun
  try {
    const parsed = await parseTextAsync(raw)
    if (run !== parseRun) return false
    const next = withLinkText(fields(), parsed)
    code.value = next.code
    vaultName.value = next.vaultName
    nostrRelays.value = next.nostrRelays
    irohRelays.value = next.irohRelays
    disabledRelays.value = next.disabledRelays
    serversFromLink.value = next.serversFromLink
    serversOpen.value = false
    if (parsed.droppedServers > 0)
      toast.error(t('onboarding.link.serversDropped'), {
        id: 'link-servers-dropped',
      })
    return true
  } catch (e: unknown) {
    if (run !== parseRun) return false
    toast.error(readError(e), { id: 'link-text-read' })
    return false
  }
}

function queueLinkText(raw: string): Promise<boolean> {
  const pending = applyLinkTextAsync(raw)
  parsePending = pending
  void pending.finally(() => {
    if (parsePending === pending) parsePending = null
  })
  return pending
}

// A pasted link (or anything longer than a code) is read as soon as it is in the field: a paste
// changes the text by more than one character, typing does not and is read when the field is left.
watch(code, (value, before) => {
  parseRun += 1
  parsePending = null
  if (serversFromLink.value && !isLinkTextInput(value)) {
    nostrRelays.value = []
    irohRelays.value = []
    disabledRelays.value = []
    serversFromLink.value = false
  }
  if (isLinkTextInput(value) && Math.abs(value.length - before.length) > 1)
    void queueLinkText(value)
})

function onFocusOut() {
  if (isLinkTextInput(code.value)) void queueLinkText(code.value)
}

// Closing the preview puts the focus back on the button that opened it.
watch(scannerOpen, async (open, wasOpen) => {
  if (!wasOpen || open) return
  await nextTick()
  document.querySelector<HTMLElement>('[data-testid="link-scan"]')?.focus()
})

async function onScan() {
  if (usesPreview.value) {
    scannerOpen.value = true
    return
  }
  try {
    await applyLinkTextAsync(await scanAsync())
  } catch (e: unknown) {
    toast.error(errString(e), { id: 'link-scan' })
  }
}

/**
 * A text the preview read: only a link code ends the scan, so a QR code of something else does
 * not close the camera the user is still holding a code in front of.
 */
async function acceptScanned(raw: string): Promise<boolean> {
  const taken = await applyLinkTextAsync(raw)
  if (taken) scannerOpen.value = false
  return taken
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
  if (isLinkTextInput(code.value)) {
    const parsed = parsePending
      ? await parsePending
      : await queueLinkText(code.value)
    if (!parsed) return
  }
  submitting.value = true
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
    toast.error(
      kind === 'InvalidInput' && reason
        ? reason
        : t(`errors.${key}`, t('errors.openFailed')),
    )
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
  serversFromLink.value = false
  added[kind].value = [...added[kind].value, url]
  serversFromLink.value = false
  return true
}

function onToggleRelay(url: string, enabled: boolean) {
  serversFromLink.value = false
  disabledRelays.value = withEnabled(disabledRelays.value, url, enabled)
  serversFromLink.value = false
}

function onRemoveRelay(kind: Kind, url: string) {
  serversFromLink.value = false
  const rest = withoutServer(added[kind].value, disabledRelays.value, url)
  added[kind].value = rest.added
  disabledRelays.value = rest.disabled
  serversFromLink.value = false
}

/** Back to the form after a failure. */
function onRetry() {
  state.value = null
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
        @focusout="onFocusOut"
      >
        <UiButton
          v-if="!scannerOpen"
          type="button"
          variant="outline"
          class="w-full"
          data-testid="link-scan"
          @click="onScan"
        >
          <Icon name="lucide:scan-qr-code" class="size-4" />
          {{ t('onboarding.link.scan') }}
        </UiButton>
        <OnboardingLinkScanner
          v-if="scannerOpen"
          :accept="acceptScanned"
          @close="scannerOpen = false"
        />
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
        <div
          v-if="serversFromLink && !serversOpen"
          class="flex items-center justify-between gap-3 rounded-md border px-4 py-3 text-sm"
          data-testid="link-servers-summary"
        >
          <span class="text-muted-foreground">
            {{
              t('onboarding.link.serversTaken', {
                count: serversTakenOver(fields()),
              })
            }}
          </span>
          <UiButton
            type="button"
            variant="ghost"
            size="sm"
            data-testid="link-servers-open"
            @click="serversOpen = true"
          >
            {{ t('onboarding.link.serversShow') }}
          </UiButton>
        </div>
        <template v-else>
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
        </template>
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
        </template>
        <template v-else-if="stateKind === 'failed'">
          <Icon name="lucide:circle-alert" class="size-8 text-destructive" />
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
