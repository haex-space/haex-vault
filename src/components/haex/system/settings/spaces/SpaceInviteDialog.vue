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
        <!-- Contact mode: multi-select contacts -->
        <template v-if="mode === 'contact'">
          <div class="flex gap-2">
            <UiSelectMenu
              v-model="selectedContactIds"
              :items="contactOptions"
              value-key="value"
              multiple
              :label="t('form.selectContacts')"
              class="flex-1"
            >
              <template #empty>
                {{ t('form.noContacts') }}
              </template>
            </UiSelectMenu>
            <UButton
              icon="i-lucide-contact"
              color="neutral"
              variant="outline"
              :title="t('form.manageContacts')"
              @click="navigateToContacts"
            />
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
          <UFormField :label="t('form.maxUses')" class="mt-3">
            <UInputNumber
              v-model="maxUses"
              :min="2"
              :max="1000"
              :step="1"
              class="w-full"
            />
          </UFormField>
        </template>

        <!-- Capabilities: horizontal layout -->
        <div class="mt-3">
          <label class="text-sm font-medium">{{ t('form.capabilityLabel') }}</label>
          <div class="flex items-center gap-4 mt-1.5">
            <UCheckbox
              :model-value="true"
              disabled
              :label="t('capabilities.read')"
            />
            <UCheckbox
              v-model="capWrite"
              :label="t('capabilities.write')"
            />
            <UCheckbox
              v-model="capInvite"
              :label="t('capabilities.invite')"
            />
          </div>
        </div>

        <!-- Include history toggle -->
        <div class="flex items-center justify-between mt-3 p-3 rounded-lg bg-gray-50 dark:bg-gray-800/50">
          <div>
            <p class="text-sm font-medium">{{ t('form.includeHistory') }}</p>
            <p class="text-xs text-muted">{{ t('form.includeHistoryHint') }}</p>
          </div>
          <UiToggle v-model="includeHistory" />
        </div>

        <!-- Expiry / deadline -->
        <UiSelectMenu
          v-model="selectedExpiry"
          :items="expiryOptions"
          :label="t('form.deadlineLabel')"
          class="w-full mt-3"
        />
        <p class="text-xs text-muted mt-1">{{ t('form.deadlineHint') }}</p>

        <!-- Endpoint selector (only for local spaces) -->
        <template v-if="isLocalSpace && spaceDevices.length > 0">
          <div class="space-y-2 mt-3">
            <label class="text-sm font-medium">{{ t('form.endpointsLabel') }}</label>
            <p class="text-xs text-muted">{{ t('form.endpointsHint') }}</p>
            <div class="flex flex-col gap-2">
              <UCheckbox
                v-for="device in spaceDevices"
                :key="device.id"
                :model-value="selectedEndpointIds.has(device.deviceEndpointId)"
                @update:model-value="toggleEndpoint(device.deviceEndpointId, $event as boolean)"
              >
                <template #label>
                  <div class="flex items-center gap-2">
                    <span class="text-sm">{{ device.deviceName }}</span>
                    <code class="text-xs text-muted">{{ device.deviceEndpointId.slice(0, 12) }}…</code>
                  </div>
                </template>
              </UCheckbox>
            </div>
          </div>
        </template>
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
import type { SelectHaexContacts } from '~/database/schemas'
import { SpaceType, SpaceCapability } from '~/database/constants'
import { buildLocalInviteLink } from '~/utils/inviteLink'

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
const peerStorageStore = usePeerStorageStore()
const { contacts } = storeToRefs(contactsStore)

const isProcessing = ref(false)
const selectedContactIds = ref<string[]>([])
const inviteLabel = ref('')
const maxUses = ref(50)
const generatedLink = ref('')
const generatedExpiresAt = ref('')
const qrCanvas = ref<HTMLCanvasElement>()

// Capability checkboxes (read is always on)
const capWrite = ref(false)
const capInvite = ref(false)
const includeHistory = ref(true)

const selectedExpiry = ref<{ label: string; value: number } | undefined>()

// Endpoint selection for local spaces
const selectedEndpointIds = ref(new Set<string>())

const isLocalSpace = computed(() => {
  const space = spacesStore.spaces.find(s => s.id === props.spaceId)
  return space?.type === SpaceType.LOCAL
})

const spaceDevices = computed(() =>
  peerStorageStore.spaceDevices.filter(d => d.spaceId === props.spaceId),
)

const selectedSpaceEndpoints = computed(() =>
  spaceDevices.value
    .filter(d => selectedEndpointIds.value.has(d.deviceEndpointId))
    .map(d => d.deviceEndpointId),
)

const selectedCapabilities = computed((): string[] => {
  const caps: string[] = [SpaceCapability.READ]
  if (capWrite.value) caps.push(SpaceCapability.WRITE)
  if (capInvite.value) caps.push(SpaceCapability.INVITE)
  return caps
})

const selectedContacts = computed<SelectHaexContacts[]>(() =>
  contacts.value.filter(c => selectedContactIds.value.includes(c.id)),
)

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
  if (!selectedExpiry.value) return false
  if (props.mode === 'contact') return selectedContacts.value.length > 0
  return true
})

const formatDate = (iso: string) => new Date(iso).toLocaleString()

const toggleEndpoint = (endpointId: string, checked: boolean) => {
  if (checked) {
    selectedEndpointIds.value.add(endpointId)
  } else {
    if (selectedEndpointIds.value.size > 1) {
      selectedEndpointIds.value.delete(endpointId)
    }
  }
}

const resetForm = () => {
  selectedContactIds.value = []
  inviteLabel.value = ''
  maxUses.value = 50
  generatedLink.value = ''
  generatedExpiresAt.value = ''
  capWrite.value = false
  capInvite.value = false
  includeHistory.value = true
  selectedExpiry.value = undefined
  selectedEndpointIds.value = new Set()
}

watch(open, async (isOpen) => {
  if (isOpen) {
    resetForm()
    const defaults = expiryOptions.value
    selectedExpiry.value = props.mode === 'open' ? defaults[2] : defaults[1]
    if (props.mode === 'contact') {
      await contactsStore.loadContactsAsync()
    }
    if (isLocalSpace.value) {
      await peerStorageStore.loadSpaceDevicesAsync()
      selectedEndpointIds.value = new Set(
        spaceDevices.value.map(d => d.deviceEndpointId),
      )
    }
  }
})

const onSubmitAsync = async () => {
  if (!canSubmit.value) return
  isProcessing.value = true

  try {
    const space = spacesStore.spaces.find(s => s.id === props.spaceId)

    if (space?.type === SpaceType.LOCAL && props.mode === 'contact') {
      // P2P push invite for local space — send to each selected contact
      for (const contact of selectedContacts.value) {
        const inviteeDid = await publicKeyToDidKeyAsync(contact.publicKey)
        await spacesStore.inviteContactToLocalSpaceAsync({
          spaceId: props.spaceId,
          contactDid: inviteeDid,
          contactEndpointId: contact.publicKey, // TODO: resolve actual EndpointId from contact
          capabilities: selectedCapabilities.value,
          includeHistory: includeHistory.value,
          expiresInSeconds: selectedExpiry.value!.value,
          spaceEndpoints: selectedSpaceEndpoints.value,
        })
      }
      add({ title: t('success.invited'), color: 'success' })
      open.value = false
    } else if (space?.type === SpaceType.LOCAL) {
      // Local link/QR invite
      const { invoke } = await import('@tauri-apps/api/core')
      const tokenId = await invoke<string>('local_delivery_create_invite', {
        spaceId: props.spaceId,
        targetDid: null,
        capability: selectedCapabilities.value[0],
        maxUses: props.mode === 'open' ? maxUses.value : 1,
        expiresInSeconds: selectedExpiry.value!.value,
        includeHistory: includeHistory.value,
      })

      generatedLink.value = buildLocalInviteLink({
        spaceId: props.spaceId,
        tokenId,
        spaceEndpoints: selectedSpaceEndpoints.value,
      })
      generatedExpiresAt.value = new Date(Date.now() + selectedExpiry.value!.value * 1000).toISOString()

      await nextTick()
      if (qrCanvas.value) {
        await QRCode.toCanvas(qrCanvas.value, generatedLink.value, {
          width: 200,
          margin: 1,
          color: { dark: '#000000', light: '#ffffff' },
        })
      }
      add({ title: t('success.linkCreated'), color: 'success' })
    } else if (props.mode === 'contact') {
      // Online space: direct invite via server — send to each selected contact
      for (const contact of selectedContacts.value) {
        const inviteeDid = await publicKeyToDidKeyAsync(contact.publicKey)
        await spacesStore.inviteMemberAsync(
          props.serverUrl,
          props.spaceId,
          inviteeDid,
          selectedCapabilities.value[0]!,
          props.identityId,
          true,
        )
      }
      add({ title: t('success.invited'), color: 'success' })
      open.value = false
    } else {
      // Online space: link or open invite token
      const result = await spacesStore.createInviteTokenAsync(
        props.serverUrl,
        props.spaceId,
        {
          capability: selectedCapabilities.value[0],
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
    contact: Kontakte einladen
    link: Einladungslink erstellen
    open: Offene Einladung
  description:
    contact: Lade Kontakte direkt in diesen Space ein
    link: Erstelle einen Link, den du per Messenger oder E-Mail teilen kannst
    open: Erstelle einen QR-Code, über den mehrere Personen beitreten können
  form:
    selectContacts: Kontakte auswählen
    noContacts: Keine Kontakte vorhanden
    manageContacts: Kontakte verwalten
    capabilityLabel: Berechtigungen
    deadlineLabel: Annahmefrist
    deadlineHint: Die Einladung verfällt, wenn sie nicht innerhalb dieser Zeit angenommen wird.
    label: Bezeichnung
    labelPlaceholderLink: z.B. Einladung für Max
    labelPlaceholderOpen: z.B. Konferenz März 2026
    maxUses: Maximale Nutzungen
    includeHistory: Bisherige Daten teilen
    includeHistoryHint: Teile alle bisherigen Daten mit dem neuen Mitglied
    endpointsLabel: Geräte in der Einladung
    endpointsHint: Wähle aus, welche deiner Geräte in der Einladung enthalten sein sollen.
  capabilities:
    read: Lesen
    write: Schreiben
    invite: Einladen
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
    contact: Invite Contacts
    link: Create Invite Link
    open: Open Invitation
  description:
    contact: Directly invite contacts to this space
    link: Create a link to share via messenger or email
    open: Create a QR code that allows multiple people to join
  form:
    selectContacts: Select contacts
    noContacts: No contacts found
    manageContacts: Manage contacts
    capabilityLabel: Permissions
    deadlineLabel: Acceptance deadline
    deadlineHint: The invitation expires if not accepted within this time.
    label: Label
    labelPlaceholderLink: e.g. Invite for Max
    labelPlaceholderOpen: e.g. Conference March 2026
    maxUses: Maximum uses
    includeHistory: Share existing data
    includeHistoryHint: Share all existing data with the new member
    endpointsLabel: Devices in invitation
    endpointsHint: Choose which of your devices should be included in the invitation.
  capabilities:
    read: Read
    write: Write
    invite: Invite
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
