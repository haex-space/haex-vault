<script setup lang="ts">
/**
 * The Blossom-Server for chat files (spec 072 FR-038, FR-080): a select of the servers HaexVault
 * brings, the ones the user added and "Keiner". Every server is shown by its name with its address
 * under it, in the select and in the add form (name first); a server added without a name shows
 * its address alone. The servers the user added can be edited (name and address) and removed from
 * the row; the ones HaexVault brings cannot, so their buttons stay but are off. A second row adds a server. The choice is the vault setting
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
const MAX_NAME_CHARS = 40

type Server = { url: string; name: string }

// Public servers that took a 20 MB encrypted blob anonymously and honoured DELETE when tested
// (spec 072 research R12, 2026-11-25); a server joins this list only after the same test. The
// first one is also the backend's default (`DEFAULT_BLOSSOM_SERVER`).
const DEFAULT_SERVER = 'https://nostr.download'
const COMMUNITY: readonly Server[] = [
  { url: DEFAULT_SERVER, name: 'Nostr.download' },
  { url: 'https://files.sovbit.host', name: 'Sovbit' },
]

const { t } = useI18n()
const { errString } = useErrorString()
const fieldLabels = useFieldLabels()
const { getPrefAsync, setPrefAsync } = usePreferences()

const selected = ref('')
const own = ref<Server[]>([])
const busy = ref(false)
let reloadPending = false
let loadGeneration = 0
const checking = ref(false)
const result = ref<ChatBlossomCheck | null>(null)
const newUrl = ref('')
const newName = ref('')
const editing = ref(false)
const editingUrl = ref('')
const editName = ref('')
const editUrl = ref('')

/** `https://files.example.com/` as `files.example.com`. */
function shown(url: string): string {
  return url.replace(/^https?:\/\//, '').replace(/\/$/, '')
}

/** The name first and the address under it; a server without a name is its address alone. */
function optionOf(server: Server): SettingsSelectOption {
  return server.name
    ? { value: server.url, label: server.name, description: shown(server.url) }
    : { value: server.url, label: shown(server.url) }
}

const options = computed<SettingsSelectOption[]>(() => {
  const list: SettingsSelectOption[] = [
    { value: '', label: t('settings.chat.server.none') },
    ...COMMUNITY.map(optionOf),
    ...own.value.map(optionOf),
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

function parseServers(raw: string | null): Server[] {
  try {
    const list: unknown = JSON.parse(raw ?? '[]')
    if (!Array.isArray(list)) return []
    return list.flatMap((item) =>
      typeof item?.url === 'string'
        ? // `badge` is what the name was called before it became the name of the server.
          [{ url: item.url, name: String(item.name ?? item.badge ?? '') }]
        : [],
    )
  } catch {
    return []
  }
}

async function loadAsync(preserveSelection = false) {
  const generation = ++loadGeneration
  try {
    // What a send uses: the default until the user chose, '' once they chose none.
    const [storedSelection, storedServers] = await Promise.all([
      invoke<string | null>('chat_blossom_server'),
      getPrefAsync({ kind: 'vault' }, SERVERS_KEY),
    ])
    // A slow initial load or preference event must not overwrite a choice made in the meantime.
    if (generation !== loadGeneration) return
    if (!preserveSelection) selected.value = storedSelection ?? ''
    own.value = parseServers(storedServers)
    result.value = null
  } catch (cause) {
    if (generation === loadGeneration) toast.error(errString(cause))
  }
}

async function finishBusy(preserveSelection = true) {
  busy.value = false
  if (reloadPending) {
    reloadPending = false
    await loadAsync(preserveSelection)
  }
}

async function chooseAsync(url: string) {
  if (url === selected.value || busy.value) return
  busy.value = true
  ++loadGeneration
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
    await finishBusy()
  }
}

async function saveOwnAsync(next: Server[]) {
  // action-exempt: the list of servers belongs to the user's own choice above.
  await setPrefAsync(
    { kind: 'vault' },
    SERVERS_KEY,
    JSON.stringify(next.map(({ url, name }) => ({ url, name }))),
  )
  own.value = next
}

async function addAsync() {
  const url = newUrl.value.trim()
  const name = newName.value.trim()
  if (!url || busy.value) return
  const known = [...COMMUNITY, ...own.value].map((server) => server.url)
  busy.value = true
  ++loadGeneration
  try {
    if (!known.includes(url)) {
      await saveOwnAsync([...own.value, { url, name }])
    }
    newUrl.value = ''
    newName.value = ''
  } catch (cause) {
    toast.error(errString(cause))
    await finishBusy()
    return
  }
  await finishBusy()
  // Adding a server is meant to use it.
  await chooseAsync(url)
}

function startEdit() {
  const server = own.value.find((entry) => entry.url === selected.value)
  if (!server || busy.value) return
  editingUrl.value = server.url
  editName.value = server.name
  editUrl.value = server.url
  editing.value = true
}

/** The address typed in the edit dialog belongs to another server already. */
const editIsDuplicate = computed(() => {
  const url = editUrl.value.trim()
  return (
    url !== editingUrl.value &&
    [...COMMUNITY, ...own.value].some((server) => server.url === url)
  )
})

async function saveEditAsync() {
  const from = editingUrl.value
  const url = editUrl.value.trim()
  const name = editName.value.trim()
  if (!url || editIsDuplicate.value || busy.value) return
  busy.value = true
  ++loadGeneration
  try {
    await saveOwnAsync(
      own.value.map((server) => (server.url === from ? { url, name } : server)),
    )
    editing.value = false
  } catch (cause) {
    toast.error(errString(cause))
    await finishBusy()
    return
  }
  await finishBusy()
  // The choice follows the server to its new address.
  if (url !== from && selected.value === from) await chooseAsync(url)
}

async function removeAsync() {
  const url = selected.value
  if (!selectedIsOwn.value || busy.value) return
  busy.value = true
  ++loadGeneration
  try {
    await saveOwnAsync(own.value.filter((server) => server.url !== url))
  } catch (cause) {
    toast.error(errString(cause))
    await finishBusy()
    return
  }
  await finishBusy()
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
  if (busy.value) {
    reloadPending = true
    return
  }
  return loadAsync()
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
            <UiButtonInfo
              :label="t('settings.chat.server.info')"
              class="size-6 text-muted-foreground"
              data-testid="settings-chat-server-info"
            />
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
    <div class="flex items-center gap-1 @lg:w-32">
      <UiButtonIcon
        :label="t('settings.chat.server.checkHint')"
        :loading="checking"
        :disabled="!selected"
        data-testid="settings-chat-server-check"
        @click="checkAsync"
      >
        <Icon name="lucide:plug-zap" class="size-4" />
      </UiButtonIcon>
      <UiButtonEdit
        :label="t('settings.chat.server.edit')"
        :disabled="busy || !selectedIsOwn"
        data-testid="settings-chat-server-edit"
        @click="startEdit"
      />
      <UiButtonDelete
        :label="t('settings.chat.server.remove')"
        :disabled="busy || !selectedIsOwn"
        data-testid="settings-chat-server-remove"
        @click="removeAsync"
      />
    </div>
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
    <SettingsField size="lg">
      <div class="flex flex-col gap-2">
        <UiInput
          v-model="newName"
          :maxlength="MAX_NAME_CHARS"
          :placeholder="t('settings.chat.server.namePlaceholder')"
          :labels="fieldLabels.input.value"
          :disabled="busy"
          data-testid="settings-chat-server-add-name"
          @keydown.enter.prevent="addAsync"
        />
        <UiInput
          id="settings-chat-server-add-url"
          v-model="newUrl"
          type="url"
          :placeholder="t('settings.chat.server.addPlaceholder')"
          :labels="fieldLabels.input.value"
          :disabled="busy"
          data-testid="settings-chat-server-add-url"
          @keydown.enter.prevent="addAsync"
        />
      </div>
    </SettingsField>
    <div class="@lg:w-32">
      <UiButtonAdd
        :label="t('settings.chat.server.add')"
        :disabled="busy || !newUrl.trim()"
        data-testid="settings-chat-server-add"
        @click="addAsync"
      />
    </div>
  </SettingsRow>

  <UiDrawerModal
    v-model:open="editing"
    :title="t('settings.chat.server.editTitle')"
  >
    <template #content>
      <div
        class="flex flex-col gap-3"
        data-testid="settings-chat-server-edit-dialog"
      >
        <UiInput
          v-model="editName"
          :maxlength="MAX_NAME_CHARS"
          :placeholder="t('settings.chat.server.namePlaceholder')"
          :labels="fieldLabels.input.value"
          :disabled="busy"
          data-testid="settings-chat-server-edit-name"
          @keydown.enter.prevent="saveEditAsync"
        />
        <UiInput
          v-model="editUrl"
          type="url"
          :placeholder="t('settings.chat.server.addPlaceholder')"
          :labels="fieldLabels.input.value"
          :disabled="busy"
          data-testid="settings-chat-server-edit-url"
          @keydown.enter.prevent="saveEditAsync"
        />
        <p
          v-if="editIsDuplicate"
          class="text-sm text-destructive"
          data-testid="settings-chat-server-edit-duplicate"
        >
          {{ t('settings.chat.server.duplicate') }}
        </p>
        <div>
          <UiButton
            :loading="busy"
            :disabled="busy || !editUrl.trim() || editIsDuplicate"
            data-testid="settings-chat-server-edit-save"
            @click="saveEditAsync"
          >
            {{ t('settings.chat.server.save') }}
          </UiButton>
        </div>
      </div>
    </template>
  </UiDrawerModal>
</template>
