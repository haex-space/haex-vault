<template>
  <UiDrawerModal
    v-model:open="open"
    :title="t('invite.title')"
    :description="t('invite.description')"
  >
    <template #content>
      <template v-if="!inviteResult">
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
          :disabled="!selectedContact || !inviteForm.role?.value"
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
import type { SpaceRole } from '@haex-space/vault-sdk'
import type { SelectHaexContacts } from '~/database/schemas'

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
const inviteResult = ref('')
const selectedContactId = ref<string>('')

const inviteForm = reactive({
  role: undefined as { label: string; value: SpaceRole } | undefined,
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

  if (props.callerRole === 'admin') {
    options.push({ label: t('roles.owner'), value: 'owner', description: t('roles.ownerDesc') })
  }
  options.push(
    { label: t('roles.member'), value: 'member', description: t('roles.memberDesc') },
    { label: t('roles.reader'), value: 'reader', description: t('roles.readerDesc') },
  )

  return options
})

const resetForm = () => {
  selectedContactId.value = ''
  inviteForm.role = undefined
  inviteResult.value = ''
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

const navigateToContacts = () => {
  open.value = false
  windowManager.openWindowAsync({
    type: 'system',
    sourceId: 'settings',
    params: { category: 'contacts' },
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
    description: Lade einen Kontakt in diesen Space ein
    selectContact: Kontakt auswählen
    noContacts: Keine Kontakte vorhanden
    manageContacts: Kontakte verwalten
    roleLabel: Rolle auswählen
    resultDescription: Teile dieses Einladungs-JSON mit der Person
    resultLabel: Einladungs-JSON
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
    copy: Kopieren
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
    resultDescription: Share this invite JSON with the person
    resultLabel: Invite JSON
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
    copy: Copy
  success:
    invited: Invitation created
    copied: Copied to clipboard
  errors:
    inviteFailed: Failed to invite member
</i18n>
