<script setup lang="ts">
/**
 * The Blossom upload key (spec 072 FR-081, US8 scenario 2): the credential of a Blossom-Server that
 * only takes files from registered Nostr keys. It is an entry of the password manager, never kept
 * elsewhere: here it is made (a new key, shown as its public key to register at the server), or an
 * entry of the user that holds a key is chosen, or it is removed again. With a key set, every
 * upload is signed with it; without one each file gets a key of its own. No secret reaches this
 * window; the backend answers the status and the public key.
 */
import { invoke } from '@tauri-apps/api/core'
import { toast } from 'vue-sonner'
import type { ChatUploadKey } from '@bindings/ChatUploadKey'
import CredentialsEntryPicker from '~/components/settings/storage/CredentialsEntryPicker.vue'

const { t } = useI18n()
const { errString } = useErrorString()

const key = ref<ChatUploadKey | null>(null)
const busy = ref(false)
const choosing = ref(false)
const chosenId = ref('')
const removing = ref(false)
let loadGeneration = 0

async function loadAsync() {
  if (busy.value) return
  const generation = ++loadGeneration
  try {
    const current = await invoke<ChatUploadKey>('chat_blossom_key')
    if (generation === loadGeneration) key.value = current
  } catch (cause) {
    if (generation === loadGeneration) toast.error(errString(cause))
  }
}

/** Runs a command that answers the new status; a failure shows as a toast and the status is read
 * again, so the page shows what is stored. */
async function runAsync(command: string, args?: Record<string, unknown>) {
  if (busy.value) return false
  busy.value = true
  ++loadGeneration
  try {
    key.value = await invoke<ChatUploadKey>(command, args)
    return true
  } catch (cause) {
    toast.error(errString(cause))
    return false
  } finally {
    busy.value = false
    void loadAsync()
  }
}

const createAsync = () => runAsync('chat_blossom_key_create')

async function chooseAsync() {
  if (!chosenId.value) return
  if (
    await runAsync('chat_blossom_key_choose', {
      args: { itemId: chosenId.value },
    })
  )
    choosing.value = false
}

async function removeAsync() {
  if (await runAsync('chat_blossom_key_remove')) removing.value = false
}

async function copyAsync(npub: string) {
  try {
    await navigator.clipboard.writeText(npub)
    toast.success(t('settings.chat.key.copied'))
  } catch (cause) {
    toast.error(errString(cause))
  }
}

const pickerSource = { command: 'chat_blossom_key_candidates', args: {} }
const status = computed(() => key.value?.status ?? 'none')

onMounted(loadAsync)
// The entry itself changes in the password manager, and the setting with the sync.
onVaultTablesChanged(['preferences', 'haex_passwords_item_details'], loadAsync)
</script>

<template>
  <SettingsRow
    :title="t('settings.chat.key.label')"
    :description="t('settings.chat.key.hint')"
  >
    <div class="flex flex-wrap justify-end gap-2">
      <template v-if="status === 'none'">
        <UiButton
          variant="outline"
          :loading="busy"
          data-testid="settings-chat-key-create"
          @click="createAsync"
        >
          {{ t('settings.chat.key.create') }}
        </UiButton>
        <UiButton
          variant="outline"
          :disabled="busy"
          data-testid="settings-chat-key-choose"
          @click="choosing = true"
        >
          {{ t('settings.chat.key.choose') }}
        </UiButton>
      </template>
      <!-- With the key shown, the trash sits beside its copy button below. -->
      <DeleteButton
        v-else-if="!(status === 'present' && key?.npub)"
        :label="t('settings.chat.key.remove')"
        :disabled="busy"
        data-testid="settings-chat-key-remove"
        @click="removing = true"
      />
    </div>
    <template #below>
      <div
        v-if="status === 'present' && key?.npub"
        class="flex flex-col gap-1"
        data-testid="settings-chat-key-present"
      >
        <p class="text-sm text-muted-foreground">
          {{ t('settings.chat.key.npubLabel') }}
        </p>
        <div class="flex items-center gap-2">
          <code
            class="min-w-0 flex-1 break-all rounded-md bg-muted px-2 py-1 text-xs"
            data-testid="settings-chat-key-npub"
            >{{ key.npub }}</code
          >
          <UiButton
            variant="ghost"
            size="icon"
            :aria-label="t('settings.chat.key.copy')"
            :tooltip="t('settings.chat.key.copy')"
            data-testid="settings-chat-key-copy"
            @click="copyAsync(key.npub)"
          >
            <Icon name="lucide:copy" class="h-4 w-4" />
          </UiButton>
          <DeleteButton
            :label="t('settings.chat.key.remove')"
            :disabled="busy"
            data-testid="settings-chat-key-remove"
            @click="removing = true"
          />
        </div>
        <p v-if="key.entry?.title" class="text-xs text-muted-foreground">
          {{ t('settings.chat.key.entry', { title: key.entry.title }) }}
        </p>
      </div>
      <p
        v-else-if="status === 'missing'"
        class="text-sm text-destructive"
        role="status"
        data-testid="settings-chat-key-missing"
      >
        {{ t('settings.chat.key.missing') }}
      </p>
      <p
        v-else-if="status === 'syncing'"
        class="text-sm text-muted-foreground"
        role="status"
        data-testid="settings-chat-key-syncing"
      >
        {{ t('settings.chat.key.syncing') }}
      </p>
    </template>
  </SettingsRow>

  <UiDrawerModal
    v-if="choosing"
    :open="choosing"
    :title="t('settings.chat.key.chooseTitle')"
    @update:open="choosing = $event"
  >
    <template #content>
      <div class="px-6 py-2" data-testid="settings-chat-key-picker">
        <CredentialsEntryPicker
          v-model="chosenId"
          :source="pickerSource"
          label-key="settings.chat.key.entryLabel"
          hint-key="settings.chat.key.onlyFitting"
        />
      </div>
    </template>
    <template #footer>
      <div class="flex justify-end gap-2">
        <UiButton variant="outline" @click="choosing = false">
          {{ t('chat.cancel') }}
        </UiButton>
        <UiButton
          :disabled="!chosenId"
          :loading="busy"
          data-testid="settings-chat-key-choose-confirm"
          @click="chooseAsync"
        >
          {{ t('settings.chat.key.chooseConfirm') }}
        </UiButton>
      </div>
    </template>
  </UiDrawerModal>

  <UiDrawerModal
    v-if="removing"
    :open="removing"
    :title="t('settings.chat.key.removeTitle')"
    @update:open="removing = $event"
  >
    <template #content>
      <p class="px-6 py-2 text-sm">
        {{ t('settings.chat.key.removeHint') }}
      </p>
    </template>
    <template #footer>
      <div class="flex justify-end gap-2">
        <UiButton variant="outline" @click="removing = false">
          {{ t('chat.cancel') }}
        </UiButton>
        <UiButton
          variant="destructive"
          :loading="busy"
          data-testid="settings-chat-key-remove-confirm"
          @click="removeAsync"
        >
          {{ t('settings.chat.key.remove') }}
        </UiButton>
      </div>
    </template>
  </UiDrawerModal>
</template>
