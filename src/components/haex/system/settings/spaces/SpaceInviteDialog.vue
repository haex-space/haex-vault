<template>
  <UiDrawerModal
    v-model:open="open"
    :title="dialogTitle"
    :description="dialogDescription"
  >
    <template #content>
      <!-- Result view: show generated link -->
      <template v-if="generatedLink">
        <div class="space-y-3">
          <UiInput
            :model-value="generatedLink"
            read-only
            :label="t('result.label')"
            with-copy-button
          />
          <div class="flex justify-center p-4 bg-white rounded-lg">
            <canvas ref="qrCanvas" />
          </div>
          <p class="text-xs text-muted text-center">
            {{ t('result.expiresAt', { date: formatDate(generatedExpiresAt) }) }}
          </p>
        </div>
      </template>

      <!-- Form view -->
      <template v-else>
        <!-- Contact mode: contact selector -->
        <template v-if="mode === 'contact'">
          <div class="flex gap-2">
            <USelectMenu
              v-model="selectedContactId"
              :items="contactOptions"
              value-key="value"
              :placeholder="t('form.selectContact')"
              class="flex-1"
            >
              <template #empty>
                {{ t('form.noContacts') }}
              </template>
            </USelectMenu>
            <UButton
              icon="i-lucide-contact"
              color="neutral"
              variant="outline"
              :title="t('form.manageContacts')"
              @click="navigateToContacts"
            />
          </div>
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
        </template>

        <!-- Link/Open mode: label -->
        <template v-if="mode === 'link' || mode === 'open'">
          <UiInput
            v-model="inviteLabel"
            :label="t('form.label')"
            :placeholder="mode === 'open' ? t('form.labelPlaceholderOpen') : t('form.labelPlaceholderLink')"
          />
        </template>

        <!-- Open mode: max uses -->
        <template v-if="mode === 'open'">
          <UiInput
            v-model.number="maxUses"
            type="number"
            :label="t('form.maxUses')"
            :min="2"
            :max="1000"
            class="mt-3"
          />
        </template>

        <!-- All modes: role + expiry -->
        <USelectMenu
          v-model="selectedCapability"
          :items="capabilityOptions"
          :placeholder="t('form.capabilityLabel')"
          class="w-full mt-3"
        />

        <USelectMenu
          v-model="selectedExpiry"
          :items="expiryOptions"
          :placeholder="t('form.expiryLabel')"
          class="w-full mt-3"
        />
      </template>
    </template>
    <template #footer>
      <div class="flex justify-between gap-4">
        <UButton
          color="neutral"
          variant="outline"
          @click="open = false"
        >
          {{ generatedLink ? t('actions.close') : t('actions.cancel') }}
        </UButton>
        <UiButton
          v-if="!generatedLink"
          :icon="mode === 'contact' ? 'i-lucide-user-plus' : 'i-lucide-link'"
          :loading="isProcessing"
          :disabled="!canSubmit"
          @click="onSubmitAsync"
        >
          {{ mode === 'contact' ? t('actions.invite') : t('actions.createLink') }}
        </UiButton>
      </div>
    </template>
  </UiDrawerModal>
</template>

<script setup lang="ts">
import QRCode from 'qrcode'
import { SettingsCategory } from '~/config/settingsCategories'
import { publicKeyToDidKeyAsync } from '@haex-space/vault-sdk'
import type { SpaceCapability } from '@haex-space/ucan'
import type { SelectHaexContacts } from '~/database/schemas'

const open = defineModel<boolean>('open', { required: true })

const props = defineProps<{
  spaceId: string
  serverUrl: string
  identityId: string
  mode: 'contact' | 'link' | 'open'
}>()

const { t } = useI18n()
const { add } = useToast()

const windowManager = useWindowManagerStore()
const spacesStore = useSpacesStore()
const contactsStore = useContactsStore()
const { contacts } = storeToRefs(contactsStore)

const isProcessing = ref(false)
const selectedContactId = ref('')
const inviteLabel = ref('')
const maxUses = ref(50)
const generatedLink = ref('')
const generatedExpiresAt = ref('')
const qrCanvas = ref<HTMLCanvasElement>()

const selectedCapability = ref<{ label: string; value: SpaceCapability } | undefined>()
const selectedExpiry = ref<{ label: string; value: number } | undefined>()

const dialogTitle = computed(() => {
  switch (props.mode) {
    case 'contact': return t('title.contact')
    case 'link': return t('title.link')
    case 'open': return t('title.open')
  }
})

const dialogDescription = computed(() => {
  switch (props.mode) {
    case 'contact': return t('description.contact')
    case 'link': return t('description.link')
    case 'open': return t('description.open')
  }
})

const contactOptions = computed(() =>
  contacts.value.map(c => ({ label: c.label, value: c.id })),
)

const selectedContact = computed<SelectHaexContacts | undefined>(() =>
  contacts.value.find(c => c.id === selectedContactId.value),
)

const capabilityOptions = computed((): { label: string; value: SpaceCapability }[] => [
  { label: t('capabilities.admin'), value: 'space/admin' },
  { label: t('capabilities.invite'), value: 'space/invite' },
  { label: t('capabilities.write'), value: 'space/write' },
  { label: t('capabilities.read'), value: 'space/read' },
])

const expiryOptions = computed(() => {
  if (props.mode === 'open') {
    return [
      { label: t('expiry.1h'), value: 60 * 60 },
      { label: t('expiry.6h'), value: 6 * 60 * 60 },
      { label: t('expiry.1d'), value: 24 * 60 * 60 },
      { label: t('expiry.3d'), value: 3 * 24 * 60 * 60 },
      { label: t('expiry.7d'), value: 7 * 24 * 60 * 60 },
    ]
  }
  return [
    { label: t('expiry.1d'), value: 24 * 60 * 60 },
    { label: t('expiry.7d'), value: 7 * 24 * 60 * 60 },
    { label: t('expiry.30d'), value: 30 * 24 * 60 * 60 },
    { label: t('expiry.90d'), value: 90 * 24 * 60 * 60 },
  ]
})

const canSubmit = computed(() => {
  if (!selectedCapability.value || !selectedExpiry.value) return false
  if (props.mode === 'contact') return !!selectedContact.value
  return true
})

const formatDate = (iso: string) => new Date(iso).toLocaleString()

const resetForm = () => {
  selectedContactId.value = ''
  inviteLabel.value = ''
  maxUses.value = 50
  generatedLink.value = ''
  generatedExpiresAt.value = ''
  selectedCapability.value = undefined
  selectedExpiry.value = undefined
}

watch(open, async (isOpen) => {
  if (isOpen) {
    resetForm()
    // Set defaults
    selectedCapability.value = capabilityOptions.value[2] // space/write
    const defaults = expiryOptions.value
    selectedExpiry.value = props.mode === 'open' ? defaults[2] : defaults[1] // 1d for open, 7d for link/contact
    if (props.mode === 'contact') {
      await contactsStore.loadContactsAsync()
    }
  }
})

const onSubmitAsync = async () => {
  if (!canSubmit.value) return
  isProcessing.value = true

  try {
    if (props.mode === 'contact') {
      // Direct invite: DID known, UCAN created immediately
      const inviteeDid = await publicKeyToDidKeyAsync(selectedContact.value!.publicKey)
      await spacesStore.inviteMemberAsync(
        props.serverUrl,
        props.spaceId,
        inviteeDid,
        selectedCapability.value!.value,
        props.identityId,
      )
      add({ title: t('success.invited'), color: 'success' })
      open.value = false
    } else {
      // Link or Open: create invite token
      const result = await spacesStore.createInviteTokenAsync(
        props.serverUrl,
        props.spaceId,
        {
          capability: selectedCapability.value!.value,
          maxUses: props.mode === 'open' ? maxUses.value : 1,
          expiresInSeconds: selectedExpiry.value!.value,
          label: inviteLabel.value || undefined,
        },
      )
      generatedLink.value = spacesStore.buildInviteLink(props.serverUrl, props.spaceId, result.tokenId)
      generatedExpiresAt.value = result.expiresAt

      await nextTick()
      if (qrCanvas.value) {
        await QRCode.toCanvas(qrCanvas.value, generatedLink.value, {
          width: 200,
          margin: 1,
          color: { dark: '#000000', light: '#ffffff' },
        })
      }

      add({ title: t('success.linkCreated'), color: 'success' })
    }
  } catch (error) {
    console.error('Invite failed:', error)
    add({
      title: t('errors.failed'),
      description: error instanceof Error ? error.message : undefined,
      color: 'error',
    })
  } finally {
    isProcessing.value = false
  }
}

const navigateToContacts = () => {
  open.value = false
  windowManager.openWindowAsync({
    type: 'system',
    sourceId: 'settings',
    params: { category: SettingsCategory.Contacts },
  })
}
</script>

<i18n lang="yaml">
de:
  title:
    contact: Kontakt einladen
    link: Einladungslink erstellen
    open: Offene Einladung
  description:
    contact: Lade einen Kontakt direkt in diesen Space ein
    link: Erstelle einen Link, den du per Messenger oder E-Mail teilen kannst
    open: Erstelle einen QR-Code, über den mehrere Personen beitreten können
  form:
    selectContact: Kontakt auswählen
    noContacts: Keine Kontakte vorhanden
    manageContacts: Kontakte verwalten
    capabilityLabel: Berechtigung
    expiryLabel: Gültigkeit
    label: Bezeichnung
    labelPlaceholderLink: z.B. Einladung für Max
    labelPlaceholderOpen: z.B. Konferenz März 2026
    maxUses: Maximale Nutzungen
  capabilities:
    admin: Admin (Vollzugriff)
    invite: Einladen (Lesen + Schreiben + Einladen)
    write: Schreiben (Lesen + Schreiben)
    read: Lesen (nur Lesen)
  expiry:
    1h: 1 Stunde
    6h: 6 Stunden
    1d: 1 Tag
    3d: 3 Tage
    7d: 7 Tage
    30d: 30 Tage
    90d: 90 Tage
  result:
    label: Einladungslink
    expiresAt: "Gültig bis: {date}"
  actions:
    invite: Einladen
    createLink: Link erstellen
    cancel: Abbrechen
    close: Schließen
  success:
    invited: Einladung gesendet
    linkCreated: Einladungslink erstellt
  errors:
    failed: Einladung fehlgeschlagen
en:
  title:
    contact: Invite Contact
    link: Create Invite Link
    open: Open Invitation
  description:
    contact: Directly invite a contact to this space
    link: Create a link to share via messenger or email
    open: Create a QR code that allows multiple people to join
  form:
    selectContact: Select contact
    noContacts: No contacts found
    manageContacts: Manage contacts
    capabilityLabel: Permission
    expiryLabel: Valid for
    label: Label
    labelPlaceholderLink: e.g. Invite for Max
    labelPlaceholderOpen: e.g. Conference March 2026
    maxUses: Maximum uses
  capabilities:
    admin: Admin (full access)
    invite: Invite (read + write + invite)
    write: Write (read + write)
    read: Read (read only)
  expiry:
    1h: 1 hour
    6h: 6 hours
    1d: 1 day
    3d: 3 days
    7d: 7 days
    30d: 30 days
    90d: 90 days
  result:
    label: Invite link
    expiresAt: "Valid until: {date}"
  actions:
    invite: Invite
    createLink: Create link
    cancel: Cancel
    close: Close
  success:
    invited: Invitation sent
    linkCreated: Invite link created
  errors:
    failed: Invitation failed
</i18n>
