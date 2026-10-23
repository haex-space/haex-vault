<script setup lang="ts">
/**
 * holzi's own invitation prompt (spec 057, US1, T043, FR-014): "[Kontakt] lädt dich zu
 * [Erweiterung] ein" with "Annehmen" and "Ablehnen". It is the only place an invitation is
 * accepted; the extension has no way to answer for the user. Several open prompts queue and
 * are asked one at a time. An answer that finds the invitation gone (withdrawn, expired, or taken
 * by another device) says so. Closing the dialog decides nothing. Mounted once in the desktop next
 * to `PermissionRequestDialog.vue`.
 */
import { toast } from 'vue-sonner'
import { useExtensionPeerInvitationsStore } from '~/stores/extensionPeerInvitations'

const { t } = useI18n()
const { errString } = useErrorString()
const store = useExtensionPeerInvitationsStore()

const busy = ref(false)
const failure = ref<string | null>(null)

watch(
  () => store.shown?.inviteId,
  () => {
    failure.value = null
  },
)

async function answerAsync(accept: boolean) {
  const invitation = store.shown
  if (!invitation || busy.value) return
  busy.value = true
  failure.value = null
  try {
    const result = await store.answerAsync(invitation.inviteId, accept)
    if (result === 'gone') toast.info(t('extensions.peers.invitation.gone'))
  } catch (error) {
    failure.value = errString(error)
  } finally {
    busy.value = false
  }
}

function onUpdateOpen(open: boolean) {
  if (!open && store.shown) store.dismiss(store.shown.inviteId)
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
    @update:open="onUpdateOpen"
  >
    <template #content>
      <p
        v-if="failure"
        class="text-sm text-destructive"
        role="alert"
        data-testid="extension-peer-invitation-failure"
      >
        {{ failure }}
      </p>
    </template>
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
