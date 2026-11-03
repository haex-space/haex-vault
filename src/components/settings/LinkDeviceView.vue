<script setup lang="ts">
/**
 * "Gerät verknüpfen" on a main device (spec 024, user story 5, FR-023, FR-024, FR-038): shows a
 * one-use code as QR and text, then — once the new device proved it — asks whether it may also
 * become a main device, with "nein" preselected and the warning about the total loss. Nothing
 * crosses before the answer. Leaving the view withdraws a code that is still shown.
 *
 * The backend says whether a link succeeded only through `sync-devices-changed`, which it sends
 * right before the status returns to `null`; a `null` without it means the link ended without a
 * new device.
 */
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { toast } from 'vue-sonner'
import type { LinkCodeInfo } from '@bindings/LinkCodeInfo'
import type { LinkingStatus } from '@bindings/LinkingStatus'

type Phase =
  | 'idle'
  | 'code'
  | 'confirm'
  | 'sending'
  | 'done'
  | 'ended'
  | 'failed'
  | 'declined'

const { t } = useI18n()
const { errString } = useErrorString()
const {
  createCodeAsync,
  cancelCodeAsync,
  confirmAsync,
  rejectAsync,
  onHostState,
} = useDeviceLink()

const phase = ref<Phase>('idle')
const code = ref<LinkCodeInfo | null>(null)
const newDeviceName = ref('')
const asMain = ref(false)
const busy = ref(false)
const now = ref(Date.now())
const copied = ref(false)
let devicesChanged = false

const qrSource = computed(() =>
  code.value
    ? `data:image/svg+xml;utf8,${encodeURIComponent(code.value.qrSvg)}`
    : '',
)
const remaining = computed(() => {
  if (!code.value) return ''
  const seconds = Math.max(
    0,
    Math.ceil((code.value.expiresAt - now.value) / 1000),
  )
  return `${Math.floor(seconds / 60)}:${String(seconds % 60).padStart(2, '0')}`
})

let ticker: ReturnType<typeof setInterval> | undefined
const stops: UnlistenFn[] = []

onMounted(async () => {
  ticker = setInterval(() => (now.value = Date.now()), 1000)
  stops.push(
    await onHostState(onStatus),
    await listen('sync-devices-changed', () => {
      if (phase.value === 'sending') devicesChanged = true
    }),
  )
})

onBeforeUnmount(() => {
  clearInterval(ticker)
  for (const stop of stops) stop()
  if (phase.value === 'code' || phase.value === 'confirm') {
    cancelCodeAsync().catch(() => {})
  }
})

/** The backend's view of the link; `null` when no code is live any more. */
function onStatus(status: LinkingStatus | null) {
  if (status?.stage === 'awaiting_confirmation') {
    newDeviceName.value = status.newDeviceName ?? ''
    asMain.value = false
    phase.value = 'confirm'
    return
  }
  if (status?.stage === 'code_shown') return
  if (phase.value === 'sending') {
    phase.value = devicesChanged ? 'done' : 'failed'
  } else if (phase.value === 'code' || phase.value === 'confirm') {
    phase.value = 'ended'
  }
}

async function onShowCode() {
  busy.value = true
  try {
    code.value = await createCodeAsync()
    copied.value = false
    phase.value = 'code'
  } catch (e) {
    const kind = (e as { kind?: string })?.kind
    toast.error(
      kind === 'NotMainDevice' ? t('settings.link.notMain') : errString(e),
    )
  } finally {
    busy.value = false
  }
}

async function onCancel() {
  phase.value = 'idle'
  code.value = null
  try {
    await cancelCodeAsync()
  } catch (e) {
    toast.error(errString(e))
  }
}

async function onConfirm() {
  busy.value = true
  devicesChanged = false
  phase.value = 'sending'
  try {
    await confirmAsync(asMain.value)
  } catch (e) {
    if (phase.value === 'sending') phase.value = 'confirm'
    toast.error(errString(e))
  } finally {
    busy.value = false
  }
}

async function onReject() {
  busy.value = true
  try {
    await rejectAsync()
    phase.value = 'declined'
  } catch (e) {
    toast.error(errString(e))
  } finally {
    busy.value = false
  }
}

async function onCopy() {
  if (!code.value) return
  try {
    await navigator.clipboard.writeText(code.value.code)
    copied.value = true
  } catch {
    copied.value = false
  }
}

function onAgain() {
  phase.value = 'idle'
  code.value = null
}
</script>

<template>
  <section class="flex flex-col gap-3" data-testid="link-device-view">
    <SettingsGroup v-if="phase === 'idle'">
      <SettingsRow
        icon="lucide:qr-code"
        :title="t('settings.link.startTitle')"
        :description="t('settings.link.startDescription')"
      >
        <UiButton
          :loading="busy"
          data-testid="link-show-code"
          @click="onShowCode"
        >
          {{ t('settings.link.showCode') }}
        </UiButton>
      </SettingsRow>
    </SettingsGroup>

    <template v-else-if="phase === 'code' && code">
      <SettingsGroup :label="t('settings.link.codeLabel')">
        <li
          class="flex flex-col items-center gap-3 px-4 py-5 text-center"
          data-testid="link-code-shown"
        >
          <img
            :src="qrSource"
            :alt="t('settings.link.qrAlt')"
            class="size-48 rounded-md bg-background p-2"
          />
          <p
            class="font-mono text-lg tracking-wider select-all"
            data-testid="link-code-text"
          >
            {{ code.code }}
          </p>
          <p class="text-sm text-muted-foreground">
            {{ t('settings.link.codeHint') }}
          </p>
          <p class="text-sm text-muted-foreground">
            {{ t('settings.link.validFor', { time: remaining }) }}
          </p>
          <div class="flex gap-2">
            <UiButton variant="outline" @click="onCopy">
              {{ copied ? t('settings.link.copied') : t('settings.link.copy') }}
            </UiButton>
            <UiButton variant="outline" @click="onCancel">
              {{ t('settings.link.cancel') }}
            </UiButton>
          </div>
        </li>
      </SettingsGroup>
    </template>

    <template v-else-if="phase === 'confirm'">
      <SettingsGroup
        radio
        :label="t('settings.link.confirmTitle', { name: newDeviceName })"
        :model-value="asMain ? 'main' : 'linked'"
        @update:model-value="asMain = $event === 'main'"
      >
        <SettingsOptionRow
          type="radio"
          value="linked"
          :title="t('settings.link.roleLinked')"
          :description="t('settings.link.roleLinkedDescription')"
          data-testid="link-role-linked"
        />
        <SettingsOptionRow
          type="radio"
          value="main"
          :title="t('settings.link.roleMain')"
          :description="t('settings.link.roleMainDescription')"
          data-testid="link-role-main"
        />
      </SettingsGroup>
      <p class="px-1 text-sm text-destructive" role="note">
        {{ t('settings.link.totalLossWarning') }}
      </p>
      <div class="flex gap-2">
        <UiButton :loading="busy" data-testid="link-confirm" @click="onConfirm">
          {{ t('settings.link.confirm') }}
        </UiButton>
        <UiButton
          variant="outline"
          :disabled="busy"
          data-testid="link-reject"
          @click="onReject"
        >
          {{ t('settings.link.reject') }}
        </UiButton>
      </div>
    </template>

    <div
      v-else
      class="flex flex-col items-center gap-3 py-8 text-center"
      role="status"
      :data-phase="phase"
    >
      <template v-if="phase === 'sending'">
        <Icon name="lucide:loader-circle" class="size-8 animate-spin" />
        <p class="font-medium">
          {{ t('settings.link.sending', { name: newDeviceName }) }}
        </p>
      </template>
      <template v-else-if="phase === 'done'">
        <Icon name="lucide:circle-check" class="size-8 text-primary" />
        <p class="font-medium">
          {{ t('settings.link.done', { name: newDeviceName }) }}
        </p>
        <UiButton variant="outline" data-testid="link-again" @click="onAgain">
          {{ t('settings.link.another') }}
        </UiButton>
      </template>
      <template v-else>
        <Icon name="lucide:circle-alert" class="size-8 text-muted-foreground" />
        <p class="font-medium">
          {{ t(`settings.link.${phase}`) }}
        </p>
        <UiButton variant="outline" data-testid="link-again" @click="onAgain">
          {{ t('settings.link.again') }}
        </UiButton>
      </template>
    </div>
  </section>
</template>
