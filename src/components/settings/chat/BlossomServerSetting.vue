<script setup lang="ts">
/**
 * The Blossom-Server for chat files (spec 072 FR-038, FR-080): none, or an address of the user's
 * own. Saved when the field loses focus or on Enter, without a save button (spec 023 FR-021); an
 * empty field means none, and files cannot be attached then. "Prüfen" asks the server whether it
 * takes files up to the largest size. Stored as the vault setting `chat.blossom.server`; every
 * send reads it again, so a change applies to the next message.
 */
import { invoke } from '@tauri-apps/api/core'
import { toast } from 'vue-sonner'
import type { ChatBlossomCheck } from '@bindings/ChatBlossomCheck'

const KEY = 'chat.blossom.server'

const { t } = useI18n()
const { errString } = useErrorString()
const fieldLabels = useFieldLabels()
const { getPrefAsync, setPrefAsync } = usePreferences()

const stored = ref('')
const address = ref('')
const busy = ref(false)
const checking = ref(false)
const result = ref<ChatBlossomCheck | null>(null)

async function loadAsync() {
  try {
    stored.value = (await getPrefAsync({ kind: 'vault' }, KEY)) ?? ''
    address.value = stored.value
    result.value = null
  } catch (cause) {
    toast.error(errString(cause))
  }
}

async function commitAsync() {
  const next = address.value.trim()
  if (next === stored.value || busy.value) return
  busy.value = true
  try {
    // action-exempt: where chat files go is the user's own choice; no agent gets an action to
    // point them at another server.
    await setPrefAsync({ kind: 'vault' }, KEY, next)
    stored.value = next
    address.value = next
    result.value = null
  } catch (cause) {
    toast.error(errString(cause))
    // Show what is stored, not what was typed.
    await loadAsync()
  } finally {
    busy.value = false
  }
}

async function checkAsync() {
  const url = address.value.trim()
  if (!url || checking.value) return
  checking.value = true
  result.value = null
  try {
    const checked = await invoke<ChatBlossomCheck>('chat_blossom_check', {
      args: { url },
    })
    if (address.value.trim() === url) result.value = checked
  } catch (cause) {
    toast.error(errString(cause))
  } finally {
    checking.value = false
  }
}

/** The line under the field for the result of the check. */
const verdictText = computed(() => {
  const found = result.value
  if (!found) return ''
  const reason = found.reason ?? ''
  const status = found.status === null ? '' : String(found.status)
  return t(`settings.chat.server.verdict.${found.verdict}`, { reason, status })
})

const verdictIsGood = computed(() => result.value?.verdict === 'accepts')

onMounted(loadAsync)
onVaultTablesChanged(['preferences'], loadAsync)
</script>

<template>
  <SettingsRow
    :title="t('settings.chat.server.label')"
    :description="t('settings.chat.server.hint')"
    label-for="settings-chat-server-input"
  >
    <div class="flex w-80 max-w-full gap-2">
      <UiInput
        id="settings-chat-server-input"
        v-model="address"
        type="url"
        :placeholder="t('settings.chat.server.placeholder')"
        :labels="fieldLabels.input.value"
        :disabled="busy"
        data-testid="settings-chat-server"
        @blur="commitAsync"
        @keydown.enter.prevent="commitAsync"
      />
      <UiButton
        variant="outline"
        :loading="checking"
        :disabled="!address.trim()"
        data-testid="settings-chat-server-check"
        @click="checkAsync"
      >
        {{ t('settings.chat.server.check') }}
      </UiButton>
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
</template>
