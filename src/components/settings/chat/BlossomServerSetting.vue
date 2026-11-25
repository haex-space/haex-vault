<script setup lang="ts">
/**
 * The Blossom-Server for chat files (spec 072 FR-038, FR-080): a select of the servers HaexVault
 * brings (badge "Community-Server"), the ones the user added (with a badge of their own, if they
 * gave one) and "Keiner". A second row adds a server. The choice is the vault setting
 * `chat.blossom.server`, the added servers `chat.blossom.servers`; saved on selection, and every
 * send reads the choice again, so a change applies to the next message. "Prüfen" asks the chosen
 * server whether it takes files up to the largest size. What the servers mean (encryption, what a
 * server sees, no guarantee) sits behind an info button, not in the row.
 */
import { invoke } from '@tauri-apps/api/core'
import { toast } from 'vue-sonner'
import type { ChatBlossomCheck } from '@bindings/ChatBlossomCheck'
import type { SettingsSelectOption } from '~/components/settings/Select.vue'

const KEY = 'chat.blossom.server'
const SERVERS_KEY = 'chat.blossom.servers'
const MAX_BADGE_CHARS = 24

type OwnServer = { url: string; badge: string }

// Public servers that took a 20 MB encrypted blob anonymously and honoured DELETE when tested
// (spec 072 research R12, 2026-11-25); a server joins this list only after the same test. The
// first one is also the backend's default (`DEFAULT_BLOSSOM_SERVER`).
const DEFAULT_SERVER = 'https://nostr.download'
const COMMUNITY = [DEFAULT_SERVER, 'https://files.sovbit.host']

const { t } = useI18n()
const { errString } = useErrorString()
const fieldLabels = useFieldLabels()
const { getPrefAsync, setPrefAsync } = usePreferences()

const selected = ref('')
const own = ref<OwnServer[]>([])
const busy = ref(false)
const checking = ref(false)
const result = ref<ChatBlossomCheck | null>(null)
const newUrl = ref('')
const newBadge = ref('')

/** `https://files.example.com/` as `files.example.com`. */
function shown(url: string): string {
  return url.replace(/^https?:\/\//, '').replace(/\/$/, '')
}

const options = computed<SettingsSelectOption[]>(() => {
  const list: SettingsSelectOption[] = [
    { value: '', label: t('settings.chat.server.none') },
    ...COMMUNITY.map((url) => ({
      value: url,
      label: shown(url),
      badges: [
        { text: t('settings.chat.server.community'), tone: 'neutral' as const },
      ],
    })),
    ...own.value.map((server) => ({
      value: server.url,
      label: shown(server.url),
      badges: server.badge
        ? [{ text: server.badge, tone: 'neutral' as const }]
        : [],
    })),
  ]
  // A server set by something else than this list stays visible instead of an empty select.
  if (selected.value && !list.some((o) => o.value === selected.value)) {
    list.push({ value: selected.value, label: shown(selected.value) })
  }
  return list
})

const selectedIsOwn = computed(() =>
  own.value.some((server) => server.url === selected.value),
)

function parseServers(raw: string | null): OwnServer[] {
  try {
    const list: unknown = JSON.parse(raw ?? '[]')
    if (!Array.isArray(list)) return []
    return list.flatMap((item) =>
      typeof item?.url === 'string'
        ? [{ url: item.url, badge: String(item.badge ?? '') }]
        : [],
    )
  } catch {
    return []
  }
}

async function loadAsync() {
  try {
    // What a send uses: the default until the user chose, '' once they chose none.
    selected.value = (await invoke<string | null>('chat_blossom_server')) ?? ''
    own.value = parseServers(await getPrefAsync({ kind: 'vault' }, SERVERS_KEY))
    result.value = null
  } catch (cause) {
    toast.error(errString(cause))
  }
}

async function chooseAsync(url: string) {
  if (url === selected.value || busy.value) return
  busy.value = true
  try {
    // action-exempt: where chat files go is the user's own choice; no agent gets an action to
    // point them at another server.
    await setPrefAsync({ kind: 'vault' }, KEY, url)
    selected.value = url
    result.value = null
  } catch (cause) {
    toast.error(errString(cause))
    // Show what is stored, not what was picked.
    await loadAsync()
  } finally {
    busy.value = false
  }
}

async function saveOwnAsync(next: OwnServer[]) {
  // action-exempt: the list of servers belongs to the user's own choice above.
  await setPrefAsync(
    { kind: 'vault' },
    SERVERS_KEY,
    JSON.stringify(next.map(({ url, badge }) => ({ url, badge }))),
  )
  own.value = next
}

async function addAsync() {
  const url = newUrl.value.trim()
  const badge = newBadge.value.trim()
  if (!url || busy.value) return
  const known = [...COMMUNITY, ...own.value.map((server) => server.url)]
  busy.value = true
  try {
    if (!known.includes(url)) {
      await saveOwnAsync([...own.value, { url, badge }])
    }
    newUrl.value = ''
    newBadge.value = ''
  } catch (cause) {
    toast.error(errString(cause))
    busy.value = false
    return
  }
  busy.value = false
  // Adding a server is meant to use it.
  await chooseAsync(url)
}

async function removeAsync() {
  const url = selected.value
  if (!selectedIsOwn.value || busy.value) return
  busy.value = true
  try {
    await saveOwnAsync(own.value.filter((server) => server.url !== url))
  } catch (cause) {
    toast.error(errString(cause))
    busy.value = false
    return
  }
  busy.value = false
  await chooseAsync(DEFAULT_SERVER)
}

async function checkAsync() {
  const url = selected.value
  if (!url || checking.value) return
  checking.value = true
  result.value = null
  try {
    const checked = await invoke<ChatBlossomCheck>('chat_blossom_check', {
      args: { url },
    })
    if (selected.value === url) result.value = checked
  } catch (cause) {
    toast.error(errString(cause))
  } finally {
    checking.value = false
  }
}

/** The line under the select for the result of the check. */
const verdictText = computed(() => {
  const found = result.value
  if (!found) return ''
  const reason = found.reason ?? ''
  const status = found.status === null ? '' : String(found.status)
  return t(`settings.chat.server.verdict.${found.verdict}`, { reason, status })
})

const verdictIsGood = computed(() => result.value?.verdict === 'accepts')

onMounted(loadAsync)
onVaultTablesChanged(['preferences'], () => {
  if (!busy.value) return loadAsync()
})
</script>

<template>
  <SettingsRow
    :title="t('settings.chat.server.label')"
    label-for="settings-chat-server"
  >
    <template #title>
      <span class="flex items-center gap-1">
        <label for="settings-chat-server">
          {{ t('settings.chat.server.label') }}
        </label>
        <ShadcnPopover>
          <ShadcnPopoverTrigger as-child>
            <UiButton
              variant="ghost"
              size="icon"
              class="size-6 text-muted-foreground"
              :aria-label="t('settings.chat.server.info')"
              data-testid="settings-chat-server-info"
            >
              <Icon name="lucide:info" class="size-4" />
            </UiButton>
          </ShadcnPopoverTrigger>
          <ShadcnPopoverContent
            align="start"
            class="w-80 text-sm"
            data-testid="settings-chat-server-info-panel"
          >
            {{ t('settings.chat.server.hint') }}
          </ShadcnPopoverContent>
        </ShadcnPopover>
      </span>
    </template>
    <SettingsSelect
      id="settings-chat-server"
      :model-value="selected"
      :options="options"
      :disabled="busy"
      class="w-64"
      data-testid="settings-chat-server"
      @update:model-value="chooseAsync"
    />
    <UiButton
      variant="outline"
      :loading="checking"
      :disabled="!selected"
      data-testid="settings-chat-server-check"
      @click="checkAsync"
    >
      {{ t('settings.chat.server.check') }}
    </UiButton>
    <DeleteButton
      v-if="selectedIsOwn"
      :label="t('settings.chat.server.remove')"
      :disabled="busy"
      data-testid="settings-chat-server-remove"
      @click="removeAsync"
    />
    <template #below>
      <p
        v-if="result"
        class="text-sm"
        :class="verdictIsGood ? 'text-foreground' : 'text-destructive'"
        role="status"
        data-testid="settings-chat-server-result"
      >
        {{ verdictText }}
      </p>
    </template>
  </SettingsRow>

  <SettingsRow
    :title="t('settings.chat.server.addTitle')"
    :description="t('settings.chat.server.addHint')"
    label-for="settings-chat-server-add-url"
  >
    <UiInput
      id="settings-chat-server-add-url"
      v-model="newUrl"
      type="url"
      class="w-64"
      :placeholder="t('settings.chat.server.addPlaceholder')"
      :labels="fieldLabels.input.value"
      :disabled="busy"
      data-testid="settings-chat-server-add-url"
      @keydown.enter.prevent="addAsync"
    />
    <UiInput
      v-model="newBadge"
      class="w-40"
      :maxlength="MAX_BADGE_CHARS"
      :placeholder="t('settings.chat.server.badgePlaceholder')"
      :labels="fieldLabels.input.value"
      :disabled="busy"
      data-testid="settings-chat-server-add-badge"
      @keydown.enter.prevent="addAsync"
    />
    <UiButton
      variant="outline"
      :disabled="busy || !newUrl.trim()"
      data-testid="settings-chat-server-add"
      @click="addAsync"
    >
      {{ t('settings.chat.server.add') }}
    </UiButton>
  </SettingsRow>
</template>
