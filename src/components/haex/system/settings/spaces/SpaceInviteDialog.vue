<template>
  <UiDrawerModal
    v-model:open="open"
    :title="t('invite.title')"
    :description="t('invite.description')"
  >
    <template #content>
      <template v-if="!inviteResult">
        <UiInput
          v-model="inviteForm.label"
          :label="t('invite.labelLabel')"
          :placeholder="t('invite.labelPlaceholder')"
        />
        <UiTextarea
          v-model="inviteForm.publicKey"
          :label="t('invite.publicKeyLabel')"
          :placeholder="t('invite.publicKeyPlaceholder')"
          :rows="3"
        />
        <USelectMenu
          v-model="inviteForm.role"
          :items="roleOptions"
          :placeholder="t('invite.roleLabel')"
          class="w-full"
        />
        <div class="flex items-center gap-2 mt-2">
          <UToggle v-model="inviteForm.canInvite" />
          <span class="text-sm">{{ t('invite.canInviteLabel') }}</span>
        </div>
      </template>
      <template v-else>
        <p class="text-sm text-gray-500 dark:text-gray-400 mb-2">
          {{ t('invite.resultDescription') }}
        </p>
        <UiTextarea
          :model-value="inviteResult"
          read-only
          :rows="10"
          :label="t('invite.resultLabel')"
        />
      </template>
    </template>
    <template #footer>
      <div class="flex justify-between gap-4">
        <UButton
          color="neutral"
          variant="outline"
          @click="closeDialog"
        >
          {{ inviteResult ? t('actions.close') : t('actions.cancel') }}
        </UButton>
        <UiButton
          v-if="!inviteResult"
          icon="i-lucide-user-plus"
          :loading="isInviting"
          :disabled="!inviteForm.publicKey || !inviteForm.label || !inviteForm.role?.value"
          @click="onInviteMemberAsync"
        >
          {{ t('actions.invite') }}
        </UiButton>
        <UiButton
          v-else
          icon="mdi:content-copy"
          @click="copyInvite"
        >
          {{ t('actions.copy') }}
        </UiButton>
      </div>
    </template>
  </UiDrawerModal>
</template>

<script setup lang="ts">
const open = defineModel<boolean>('open', { required: true })

const props = defineProps<{
  spaceId: string
  serverUrl: string
  isAdmin: boolean
}>()

const { t } = useI18n()
const { add } = useToast()
const { copy } = useClipboard()

const spacesStore = useSpacesStore()

const isInviting = ref(false)
const inviteResult = ref('')

const inviteForm = reactive({
  publicKey: '',
  label: '',
  role: undefined as { label: string; value: string } | undefined,
  canInvite: false,
})

const roleOptions = computed(() => [
  { label: t('roles.member'), value: 'member' },
  { label: t('roles.viewer'), value: 'viewer' },
])

const resetForm = () => {
  inviteForm.publicKey = ''
  inviteForm.label = ''
  inviteForm.role = undefined
  inviteForm.canInvite = false
  inviteResult.value = ''
}

watch(open, (isOpen) => {
  if (isOpen) {
    resetForm()
  }
})

const onInviteMemberAsync = async () => {
  if (!inviteForm.publicKey || !inviteForm.label || !inviteForm.role?.value || !props.spaceId) return

  isInviting.value = true
  try {
    const invite = await spacesStore.inviteMemberAsync(
      props.serverUrl,
      props.spaceId,
      inviteForm.publicKey.trim(),
      inviteForm.label,
      inviteForm.role.value as 'member' | 'viewer',
      inviteForm.canInvite,
    )

    inviteResult.value = JSON.stringify(invite, null, 2)

    add({
      title: t('success.invited'),
      color: 'success',
    })
  } catch (error) {
    console.error('Failed to invite member:', error)
    add({
      title: t('errors.inviteFailed'),
      description: error instanceof Error ? error.message : 'Unknown error',
      color: 'error',
    })
  } finally {
    isInviting.value = false
  }
}

const copyInvite = () => {
  copy(inviteResult.value)
  add({
    title: t('success.copied'),
    color: 'success',
  })
}

const closeDialog = () => {
  open.value = false
  inviteResult.value = ''
}
</script>

<i18n lang="yaml">
de:
  invite:
    title: Mitglied einladen
    description: Lade jemanden in diesen Space ein
    labelLabel: Name
    labelPlaceholder: z.B. Alice, Team-Lead, ...
    publicKeyLabel: Public Key
    publicKeyPlaceholder: Base64-kodierten Public Key einfügen
    roleLabel: Rolle auswählen
    canInviteLabel: Darf weitere Mitglieder einladen
    resultDescription: Teile dieses Einladungs-JSON mit der Person
    resultLabel: Einladungs-JSON
  roles:
    member: Mitglied
    viewer: Betrachter
  actions:
    invite: Einladen
    cancel: Abbrechen
    close: Schließen
    copy: Kopieren
  success:
    invited: Einladung erstellt
    copied: In Zwischenablage kopiert
  errors:
    inviteFailed: Einladung fehlgeschlagen
en:
  invite:
    title: Invite Member
    description: Invite someone to this space
    labelLabel: Name
    labelPlaceholder: e.g. Alice, Team Lead, ...
    publicKeyLabel: Public Key
    publicKeyPlaceholder: Paste Base64-encoded public key
    roleLabel: Select role
    canInviteLabel: Can invite other members
    resultDescription: Share this invite JSON with the person
    resultLabel: Invite JSON
  roles:
    member: Member
    viewer: Viewer
  actions:
    invite: Invite
    cancel: Cancel
    close: Close
    copy: Copy
  success:
    invited: Invitation created
    copied: Copied to clipboard
  errors:
    inviteFailed: Failed to invite member
</i18n>
