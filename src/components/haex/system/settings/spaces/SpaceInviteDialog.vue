<template>
  <UiDrawerModal
    v-model:open="open"
    :title="t('invite.title')"
    :description="t('invite.description')"
  >
    <template #content>
      <template v-if="!inviteLink">
        <!-- Contact selection -->
        <div class="flex gap-2">
          <USelectMenu
            v-model="selectedContactId"
            :items="contactOptions"
            value-key="value"
            :placeholder="t('invite.selectContact')"
            class="flex-1"
          >
            <template #empty>
              {{ t('invite.noContacts') }}
            </template>
          </USelectMenu>
          <UButton
            icon="i-lucide-contact"
            color="neutral"
            variant="outline"
            :title="t('invite.manageContacts')"
            @click="navigateToContacts"
          />
        </div>

        <!-- Selected contact info -->
        <div
          v-if="selectedContact"
          class="mt-3 p-3 rounded-lg bg-gray-50 dark:bg-gray-800/50"
        >
          <div class="flex items-center gap-2">
            <UIcon name="i-lucide-user" class="w-4 h-4 text-primary shrink-0" />
            <span class="font-medium text-sm">{{ selectedContact.label }}</span>
          </div>
          <code class="block text-xs text-muted mt-1 truncate">
            {{ selectedContact.publicKey }}
          </code>
        </div>

        <!-- Role selection -->
        <USelectMenu
          v-model="inviteForm.role"
          :items="roleOptions"
          :placeholder="t('invite.roleLabel')"
          class="w-full mt-3"
        />
      </template>
      <template v-else>
        <p class="text-sm text-muted mb-3">
          {{ t('invite.resultDescription') }}
        </p>

        <!-- QR Code -->
        <div class="flex justify-center p-4 bg-white rounded-lg">
          <canvas ref="qrCanvas" />
        </div>

        <!-- Invite Link -->
        <div class="mt-3">
          <UiInput
            :model-value="inviteLink"
            read-only
            :label="t('invite.resultLabel')"
            with-copy-button
          />
        </div>
      </template>
    </template>
    <template #footer>
      <div class="flex justify-between gap-4">
        <UButton
          color="neutral"
          variant="outline"
          @click="closeDialog"
        >
          {{ inviteLink ? t('actions.close') : t('actions.cancel') }}
        </UButton>
        <UiButton
          v-if="!inviteLink"
          icon="i-lucide-user-plus"
          :loading="isInviting"
          :disabled="!selectedContact || !inviteForm.role?.value"
          @click="onInviteMemberAsync"
        >
          {{ t('actions.invite') }}
        </UiButton>
        <UiButton
          v-else
          icon="mdi:content-copy"
          @click="copyInviteLink"
        >
          {{ t('actions.copyLink') }}
        </UiButton>
      </div>
    </template>
  </UiDrawerModal>
</template>

<script setup lang="ts">
import { SettingsCategory } from '~/config/settingsCategories'
import QRCode from 'qrcode'
import { SpaceRoles, type SpaceRole } from '@haex-space/vault-sdk'
import type { SelectHaexContacts } from '~/database/schemas'
import { encodeInviteLink } from '~/utils/inviteLink'

const open = defineModel<boolean>('open', { required: true })

const props = defineProps<{
  spaceId: string
  serverUrl: string
  callerRole: SpaceRole
  identityId: string
}>()

const { t } = useI18n()
const { add } = useToast()
const { copy } = useClipboard()

const windowManager = useWindowManagerStore()
const spacesStore = useSpacesStore()
const contactsStore = useContactsStore()
const { contacts } = storeToRefs(contactsStore)

const isInviting = ref(false)
const inviteLink = ref('')
const selectedContactId = ref<string>('')
const qrCanvas = ref<HTMLCanvasElement>()


const inviteForm = reactive({
  role: undefined as { label: string; value: SpaceRole; description: string } | undefined,
})

const contactOptions = computed(() =>
  contacts.value.map(c => ({
    label: c.label,
    value: c.id,
  })),
)

const selectedContact = computed<SelectHaexContacts | undefined>(() =>
  contacts.value.find(c => c.id === selectedContactId.value),
)

const roleOptions = computed(() => {
  const options: { label: string; value: SpaceRole; description: string }[] = []

  if (props.callerRole === SpaceRoles.ADMIN) {
    options.push({ label: t('roles.owner'), value: SpaceRoles.OWNER, description: t('roles.ownerDesc') })
  }
  options.push(
    { label: t('roles.member'), value: SpaceRoles.MEMBER, description: t('roles.memberDesc') },
    { label: t('roles.reader'), value: SpaceRoles.READER, description: t('roles.readerDesc') },
  )

  return options
})

const resetForm = () => {
  selectedContactId.value = ''
  inviteForm.role = undefined
  inviteLink.value = ''
}

watch(open, async (isOpen) => {
  if (isOpen) {
    resetForm()
    await contactsStore.loadContactsAsync()
  }
})

const onInviteMemberAsync = async () => {
  if (!selectedContact.value || !inviteForm.role?.value || !props.spaceId) return

  isInviting.value = true
  try {
    const invite = await spacesStore.inviteMemberAsync(
      props.serverUrl,
      props.spaceId,
      selectedContact.value.publicKey,
      selectedContact.value.label,
      inviteForm.role.value,
      props.identityId,
    )

    inviteLink.value = encodeInviteLink(invite)

    await nextTick()
    if (qrCanvas.value) {
      await QRCode.toCanvas(qrCanvas.value, inviteLink.value, {
        width: 200,
        margin: 1,
        color: { dark: '#000000', light: '#ffffff' },
      })
    }

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

const copyInviteLink = () => {
  copy(inviteLink.value)
  add({
    title: t('success.copied'),
    color: 'success',
  })
}

const navigateToContacts = () => {
  open.value = false
  windowManager.openWindowAsync({
    type: 'system',
    sourceId: 'settings',
    params: { category: SettingsCategory.Contacts },
  })
}

const closeDialog = () => {
  open.value = false
  inviteLink.value = ''
}
</script>

<i18n lang="yaml">
de:
  invite:
    title: Mitglied einladen
    description: Lade einen Kontakt in diesen Space ein
    selectContact: Kontakt auswählen
    noContacts: Keine Kontakte vorhanden
    manageContacts: Kontakte verwalten
    roleLabel: Rolle auswählen
    resultDescription: Teile diesen Einladungslink oder scanne den QR-Code
    resultLabel: Einladungslink
  roles:
    owner: Eigentümer
    ownerDesc: Vollzugriff inkl. Space-Verwaltung und Mitglieder-Einladung
    member: Mitglied
    memberDesc: Kann Inhalte lesen, erstellen und bearbeiten
    reader: Leser
    readerDesc: Kann Inhalte nur lesen, keine Änderungen möglich
  actions:
    invite: Einladen
    cancel: Abbrechen
    close: Schließen
    copyLink: Link kopieren
  success:
    invited: Einladung erstellt
    copied: In Zwischenablage kopiert
  errors:
    inviteFailed: Einladung fehlgeschlagen
en:
  invite:
    title: Invite Member
    description: Invite a contact to this space
    selectContact: Select contact
    noContacts: No contacts found
    manageContacts: Manage contacts
    roleLabel: Select role
    resultDescription: Share this invite link or scan the QR code
    resultLabel: Invite link
  roles:
    owner: Owner
    ownerDesc: Full access including space management and member invitations
    member: Member
    memberDesc: Can read, create, and edit content
    reader: Reader
    readerDesc: Read-only access, no modifications allowed
  actions:
    invite: Invite
    cancel: Cancel
    close: Close
    copyLink: Copy link
  success:
    invited: Invitation created
    copied: Copied to clipboard
  errors:
    inviteFailed: Failed to invite member
</i18n>
