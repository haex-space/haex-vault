<script setup lang="ts">
/**
 * The blocked vaults (spec 051 FR-039): their identity and since when; unblocking is one press.
 * Hidden while none is blocked.
 */
import { toast } from 'vue-sonner'
import type { ContactsBlockedVault } from '@bindings/ContactsBlockedVault'

const { t, d } = useI18n()
const { errString } = useErrorString()
const { blockedAsync, unblockAsync } = useContactCards()

const blocked = ref<ContactsBlockedVault[]>([])

async function loadAsync() {
  try {
    blocked.value = await blockedAsync()
  } catch (cause) {
    toast.error(errString(cause))
  }
}
onMounted(loadAsync)
onVaultTablesChanged(['haex_contacts_blocked_vaults'], loadAsync)

async function unblock(vault: ContactsBlockedVault) {
  try {
    // action-exempt: the contact actions join the catalog with spec 051 US6 (T083); until then the
    // button in the settings calls the command.
    await unblockAsync(vault.vaultPubkey)
    await loadAsync()
  } catch (cause) {
    toast.error(errString(cause))
  }
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
      :title="vault.npub"
      :description="
        t('settings.contacts.blocked.since', {
          at: d(new Date(vault.blockedAt), { dateStyle: 'medium' }),
        })
      "
    >
      <UiButton
        size="sm"
        variant="outline"
        :data-testid="`contacts-unblock-${vault.vaultPubkey}`"
        @click="unblock(vault)"
      >
        {{ t('settings.contacts.blocked.unblock') }}
      </UiButton>
    </SettingsRow>
  </SettingsGroup>
</template>
