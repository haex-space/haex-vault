<script setup lang="ts">
/**
 * The blocked vaults (spec 051 FR-039, spec 099 FR-015, FR-016): the hidden contact's or the kept
 * card's name and the address, since when; "Verlauf ansehen" opens the conversation, "Entsperren"
 * allows the vault again. Hidden while none is blocked.
 */
import { toast } from 'vue-sonner'
import type { ContactsBlockedVault } from '@bindings/ContactsBlockedVault'
import { contactChatPath } from '~/composables/useChatNavigation'

const { t, d } = useI18n()
const { errString } = useErrorString()
const { blockedAsync, unblockAsync } = useContactCards()
const wm = useWindowManagerStore()

const blocked = ref<ContactsBlockedVault[]>([])

async function loadAsync() {
  try {
    blocked.value = await blockedAsync()
  } catch (cause) {
    toast.error(errString(cause))
  }
}
onMounted(loadAsync)
onVaultTablesChanged(
  ['haex_contacts_blocked_vaults', 'haex_contacts*'],
  loadAsync,
)

async function unblock(vault: ContactsBlockedVault) {
  try {
    await unblockAsync(vault.vaultPubkey)
    await loadAsync()
  } catch (cause) {
    toast.error(errString(cause))
  }
}

function showHistory(threadId: string) {
  void wm.runAction(
    'wm.app.open',
    { appId: 'system.chat', at: contactChatPath(threadId) },
    { kind: 'user' },
  )
}
</script>

<template>
  <SettingsGroup
    v-if="blocked.length"
    :label="t('settings.contacts.blocked.title')"
    data-testid="contacts-blocked"
  >
    <SettingsRow
      v-for="vault in blocked"
      :key="vault.vaultPubkey"
      :title="vault.name ?? vault.npub"
      :description="
        [
          vault.name ? vault.npub : null,
          t('settings.contacts.blocked.since', {
            at: d(new Date(vault.blockedAt), { dateStyle: 'medium' }),
          }),
        ]
          .filter(Boolean)
          .join(' · ')
      "
    >
      <div class="flex flex-wrap gap-2">
        <UiButton
          v-if="vault.threadId"
          size="sm"
          variant="ghost"
          :data-testid="`contacts-blocked-history-${vault.vaultPubkey}`"
          @click="showHistory(vault.threadId)"
        >
          {{ t('settings.contacts.blocked.history') }}
        </UiButton>
        <UiButton
          size="sm"
          variant="outline"
          :data-testid="`contacts-unblock-${vault.vaultPubkey}`"
          @click="unblock(vault)"
        >
          {{ t('settings.contacts.blocked.unblock') }}
        </UiButton>
      </div>
    </SettingsRow>
  </SettingsGroup>
</template>
