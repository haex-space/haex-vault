<script setup lang="ts">
/**
 * holzi's own invitation prompt (spec 057, US1, T043, FR-014): "[Kontakt] lädt dich zu
 * [Erweiterung] ein" with "Annehmen" and "Ablehnen". It is the only place an invitation is
 * accepted; the extension has no way to answer for the user. Several open prompts queue and
 * are asked one at a time. An answer that finds the invitation gone (withdrawn, expired, or taken
 * by another device) says so. It closes only through its own buttons (spec 065 FR-042): a click
 * that only brings the window forward must not decline; "Ablehnen" lets the inviter hear
 * "declined". Mounted once in the desktop next to `PermissionRequestDialog.vue`.
 */
import { toast } from 'vue-sonner'
import { useExtensionPeerInvitationsStore } from '~/stores/extensionPeerInvitations'

const { t } = useI18n()
const { errString } = useErrorString()
const store = useExtensionPeerInvitationsStore()

const busy = ref(false)

async function answerAsync(accept: boolean) {
  const invitation = store.shown
  if (!invitation || busy.value) return
  busy.value = true
  try {
    const result = await store.answerAsync(invitation.inviteId, accept)
    if (result === 'gone') toast.info(t('extensions.peers.invitation.gone'))
  } catch (error) {
    toast.error(errString(error))
  } finally {
    // Stays set until the dialog of the next invitation is drawn, so a close the dialog reports
    // while the answered one leaves cannot decline the next.
    await nextTick()
    busy.value = false
  }
}
</script>

<template>
  <UiDrawerModal
    v-if="store.shown"
    :open="true"
    :title="
      t('extensions.peers.invitation.title', {
        contact: store.shown.contactName,
        extension: store.shown.extensionName,
      })
    "
    :dismissible="false"
  >
    <template #footer>
      <div class="flex justify-end gap-2">
        <UiButton
          size="sm"
          variant="outline"
          :disabled="busy"
          data-testid="extension-peer-invitation-decline"
          @click="answerAsync(false)"
        >
          {{ t('extensions.peers.invitation.decline') }}
        </UiButton>
        <UiButton
          size="sm"
          :disabled="busy"
          data-testid="extension-peer-invitation-accept"
          @click="answerAsync(true)"
        >
          {{ t('extensions.peers.invitation.accept') }}
        </UiButton>
      </div>
    </template>
  </UiDrawerModal>
</template>
