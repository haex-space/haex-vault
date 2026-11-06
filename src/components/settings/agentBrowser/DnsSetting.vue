<script setup lang="ts">
/**
 * "Namensauflösung" in "KI → Browser" (spec 065 FR-048, research R18): how the agent browser
 * looks up website names, through DNS over HTTPS. The mode (off, automatic, strict), the provider
 * and, for an own provider, its https address. A change applies on selection (023 FR-021) and runs
 * the guardrail action `settings.agentBrowser.dns.set`, which no agent can call; Rust checks the
 * choice again and keeps it on this device. It takes effect at the browser's next start. A refused
 * address is said under its field, any other failure is a toast.
 */
import { invoke } from '@tauri-apps/api/core'
import { toast } from 'vue-sonner'
import type { DnsMode } from '~/types/bindings/DnsMode'
import type { DnsOverview } from '~/types/bindings/DnsOverview'
import type { DnsProvider } from '~/types/bindings/DnsProvider'
import type { DnsSettings } from '~/types/bindings/DnsSettings'

const { t } = useI18n()
const { errString } = useErrorString()
const { input: inputFieldLabels } = useFieldLabels()
const setDns = useActionOrThrow('settings.agentBrowser.dns.set')

const overview = ref<DnsOverview | null>(null)
/** "Own address" chosen but no address saved yet: only the field shows, nothing is sent. */
const choosingCustom = ref(false)
const customDraft = ref('')
const customError = ref('')

async function reloadAsync() {
  try {
    overview.value = await invoke<DnsOverview>('agent_browser_dns')
    customDraft.value = overview.value.settings.custom
  } catch (error) {
    toast.error(errString(error))
  }
}

onMounted(reloadAsync)

const settings = computed<DnsSettings | null>(
  () => overview.value?.settings ?? null,
)

/** Whether names go to a provider at all. */
const active = computed(() => settings.value?.mode !== 'off')

const shownProvider = computed<DnsProvider | null>(() =>
  choosingCustom.value ? 'custom' : (settings.value?.provider ?? null),
)

const modes = computed(() =>
  (['automatic', 'secure', 'off'] as const).map((value) => ({
    value,
    label: t(`agentBrowser.settings.dns.mode.${value}`),
  })),
)

const providers = computed(() => [
  ...(overview.value?.presets ?? []).map((preset) => ({
    value: preset.id,
    label: preset.name,
  })),
  { value: 'custom', label: t('agentBrowser.settings.dns.custom') },
])

/** Changes wait for each other: each one starts from the state the one before it left. */
let queue: Promise<unknown> = Promise.resolve()

/**
 * Sends the choice `change` makes of the one shown; the error of a refusal, or `''`.
 * `change` runs when it is its turn, so two quick changes do not undo each other.
 */
function changeAsync(
  change: (choice: DnsSettings) => DnsSettings,
): Promise<string> {
  const run = async (): Promise<string> => {
    const current = settings.value
    if (!current) return ''
    try {
      overview.value = (await setDns({
        settings: change(current),
      })) as DnsOverview
      return ''
    } catch (error) {
      const draft = customDraft.value
      await reloadAsync()
      customDraft.value = draft
      return errString(error)
    }
  }
  const result = queue.then(run)
  queue = result
  return result
}

async function toastFailureAsync(failure: Promise<string>) {
  const error = await failure
  if (error) toast.error(error)
}

function setMode(mode: DnsMode) {
  void toastFailureAsync(changeAsync((choice) => ({ ...choice, mode })))
}

function setProvider(provider: DnsProvider) {
  customError.value = ''
  if (provider === 'custom') {
    const saved = settings.value?.custom ?? ''
    if (!saved) {
      choosingCustom.value = true
      return
    }
    choosingCustom.value = false
    void toastFailureAsync(
      changeAsync((choice) => ({ ...choice, provider, custom: saved })),
    )
    return
  }
  choosingCustom.value = false
  void toastFailureAsync(
    changeAsync((choice) => ({ ...choice, provider, custom: '' })),
  )
}

async function saveCustomAsync() {
  const custom = customDraft.value.trim()
  if (!custom || custom === settings.value?.custom) return
  const error = await changeAsync((choice) => ({
    ...choice,
    provider: 'custom',
    custom,
  }))
  customError.value = error
  if (!error) choosingCustom.value = false
}
</script>

<template>
  <template v-if="settings">
    <p class="px-1 text-sm font-medium">
      {{ t('agentBrowser.settings.dns.title') }}
    </p>
    <SettingsGroup data-testid="agent-browser-dns">
      <SettingsRow
        :title="t('agentBrowser.settings.dns.modeTitle')"
        :description="t(`agentBrowser.settings.dns.modeHint.${settings.mode}`)"
        label-for="agent-browser-dns-mode"
      >
        <SettingsSelect
          id="agent-browser-dns-mode"
          :model-value="settings.mode"
          class="w-48"
          :options="modes"
          data-testid="agent-browser-dns-mode"
          @update:model-value="setMode($event as DnsMode)"
        />
      </SettingsRow>
      <template v-if="active">
        <SettingsRow
          :title="t('agentBrowser.settings.dns.provider')"
          label-for="agent-browser-dns-provider"
        >
          <SettingsSelect
            id="agent-browser-dns-provider"
            :model-value="shownProvider ?? 'quad9'"
            class="w-48"
            :options="providers"
            data-testid="agent-browser-dns-provider"
            @update:model-value="setProvider($event as DnsProvider)"
          />
        </SettingsRow>
        <div v-if="shownProvider === 'custom'" class="flex flex-col gap-2 p-4">
          <UiInput
            id="agent-browser-dns-custom"
            v-model="customDraft"
            :label="t('agentBrowser.settings.dns.address')"
            :labels="inputFieldLabels"
            label-bg="var(--muted)"
            type="url"
            autocomplete="off"
            placeholder="https://dns.example/dns-query"
            :aria-invalid="customError ? 'true' : undefined"
            data-testid="agent-browser-dns-custom"
            @input="customError = ''"
            @blur="saveCustomAsync"
            @keydown.enter.prevent="saveCustomAsync"
          />
          <p
            v-if="customError"
            class="text-xs text-destructive"
            role="alert"
            data-testid="agent-browser-dns-custom-error"
          >
            {{ customError }}
          </p>
        </div>
      </template>
      <div class="flex flex-col gap-1 p-4 text-xs text-muted-foreground">
        <p>{{ t('agentBrowser.settings.dns.scope') }}</p>
        <template v-if="active">
          <p>{{ t('agentBrowser.settings.dns.providerSees') }}</p>
          <p>{{ t('agentBrowser.settings.dns.intranet') }}</p>
        </template>
      </div>
    </SettingsGroup>
  </template>
</template>
