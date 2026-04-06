<template>
  <UiDrawerModal
    v-model:open="open"
    :title="dialogTitle"
    :description="dialogDescription"
  >
    <template #body>
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
              data-testid="invite-contact-select"
            >
              <template #empty>
                {{ t('form.noContacts') }}
              </template>
            </UiSelectMenu>
            <UiButton
              icon="i-lucide-contact"
              color="neutral"
              variant="outline"
              :title="t('form.manageContacts')"
              @click="navigateToContacts"
            />
          </div>
        </template>

        <!-- Link mode: label + max uses -->
        <template v-if="mode === 'link'">
          <UiInput
            v-model="inviteLabel"
            :label="t('form.label')"
            :placeholder="t('form.labelPlaceholder')"
          />
          <UFormField :label="t('form.maxUses')" class="mt-3">
            <UInputNumber
              v-model="maxUses"
              :min="1"
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
              data-testid="invite-cap-write"
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
          :search-input="false"
          :label="t('form.deadlineLabel')"
          class="w-full mt-3"
        />
        <p class="text-xs text-muted mt-1">{{ t('form.deadlineHint') }}</p>

        <!-- Endpoint selector (only for local spaces) -->
        <template v-if="isLocalSpace && spaceDevices.length > 0">
          <UiSelectMenu
            v-model="selectedDeviceIds"
            :items="deviceOptions"
            value-key="value"
            multiple
            :label="t('form.endpointsLabel')"
            class="w-full mt-3"
          >
            <template #item="{ item }">
              <div class="flex items-center gap-2">
                <UiAvatar
                  :src="item.avatar"
                  :seed="item.endpointId"
                  size="xs"
                />
                <span class="text-sm">{{ item.label }}</span>
              </div>
            </template>
          </UiSelectMenu>
          <p class="text-xs text-muted mt-1">{{ t('form.endpointsHint') }}</p>
        </template>
      </template>
    </template>
    <template #footer>
      <div class="flex justify-between gap-4">
        <UiButton
          color="neutral"
          variant="outline"
          @click="open = false"
        >
          {{ generatedLink ? t('actions.close') : t('actions.cancel') }}
        </UiButton>
        <UiButton
          v-if="!generatedLink"
          :icon="mode === 'contact' ? 'i-lucide-user-plus' : 'i-lucide-link'"
          :loading="isProcessing"
          :disabled="!canSubmit"
          data-testid="invite-submit"
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
import type { SelectHaexIdentities } from '~/database/schemas'
import { SpaceType, SpaceCapability } from '~/database/constants'
import { buildLocalInviteLink } from '~/utils/inviteLink'
import { createLogger } from '@/stores/logging'

const log = createLogger('SPACES:INVITE-UI')

const open = defineModel<boolean>('open', { required: true })

const props = defineProps<{
  spaceId: string
  serverUrl: string
  identityId: string
  mode: 'contact' | 'link'
}>()

const { t } = useI18n()
const { add } = useToast()

const windowManager = useWindowManagerStore()
const spacesStore = useSpacesStore()
const identityStore = useIdentityStore()
const peerStorageStore = usePeerStorageStore()
const { contacts } = storeToRefs(identityStore)

const isProcessing = ref(false)
const selectedContactIds = ref<string[]>([])
const inviteLabel = ref('')
const maxUses = ref(1)
const generatedLink = ref('')
const generatedExpiresAt = ref('')
const qrCanvas = ref<HTMLCanvasElement>()

// Capability checkboxes (read is always on)
const capWrite = ref(false)
const capInvite = ref(false)
const includeHistory = ref(true)

const selectedExpiry = ref<{ label: string; value: number } | undefined>()

// Endpoint selection for local spaces
const selectedDeviceIds = ref<string[]>([])

const isLocalSpace = computed(() => {
  const space = spacesStore.spaces.find(s => s.id === props.spaceId)
  return space?.type === SpaceType.LOCAL
})

const spaceDevices = computed(() =>
  peerStorageStore.spaceDevices.filter(d => d.spaceId === props.spaceId),
)

const deviceOptions = computed(() =>
  spaceDevices.value.map(d => ({
    label: d.deviceName,
    value: d.id,
    avatar: d.avatar,
    endpointId: d.deviceEndpointId,
  })),
)

const selectedSpaceEndpoints = computed(() =>
  spaceDevices.value
    .filter(d => selectedDeviceIds.value.includes(d.id))
    .map(d => d.deviceEndpointId),
)

const selectedCapabilities = computed((): string[] => {
  const caps: string[] = [SpaceCapability.READ]
  if (capWrite.value) caps.push(SpaceCapability.WRITE)
  if (capInvite.value) caps.push(SpaceCapability.INVITE)
  return caps
})

const selectedContacts = computed<SelectHaexIdentities[]>(() =>
  contacts.value.filter(c => selectedContactIds.value.includes(c.id)),
)

const dialogTitle = computed(() =>
  props.mode === 'contact' ? t('title.contact') : t('title.link'),
)

const dialogDescription = computed(() =>
  props.mode === 'contact' ? t('description.contact') : t('description.link'),
)

const contactOptions = computed(() =>
  contacts.value.map(c => ({ label: c.label, value: c.id })),
)

const expiryOptions = computed(() => [
  { label: t('expiry.1d'), value: 24 * 60 * 60 },
  { label: t('expiry.7d'), value: 7 * 24 * 60 * 60 },
  { label: t('expiry.30d'), value: 30 * 24 * 60 * 60 },
  { label: t('expiry.90d'), value: 90 * 24 * 60 * 60 },
])

const canSubmit = computed(() => {
  if (!selectedExpiry.value) return false
  if (props.mode === 'contact') return selectedContacts.value.length > 0
  return true
})

const formatDate = (iso: string) => new Date(iso).toLocaleString()

const resetForm = () => {
  selectedContactIds.value = []
  inviteLabel.value = ''
  maxUses.value = 1
  generatedLink.value = ''
  generatedExpiresAt.value = ''
  capWrite.value = false
  capInvite.value = false
  includeHistory.value = true
  selectedExpiry.value = undefined
  selectedDeviceIds.value = []
}

watch(open, async (isOpen) => {
  if (isOpen) {
    resetForm()
    selectedExpiry.value = expiryOptions.value[1]
    if (props.mode === 'contact') {
      await identityStore.loadIdentitiesAsync()
    }
    await peerStorageStore.loadSpaceDevicesAsync()
    selectedDeviceIds.value = spaceDevices.value.map(d => d.id)
  }
})

const onSubmitAsync = async () => {
  if (!canSubmit.value) return
  isProcessing.value = true

  const capabilities = selectedCapabilities.value.join(', ')
  log.info(`Invite submit: mode=${props.mode}, space=${props.spaceId}, capabilities=[${capabilities}], expiry=${selectedExpiry.value?.value}s`)

  try {
    const space = spacesStore.spaces.find(s => s.id === props.spaceId)

    if (props.mode === 'contact') {
      // Contact invite — dual-channel: always QUIC + server if available
      log.info(`Inviting ${selectedContacts.value.length} contact(s) to space ${props.spaceId}`)
      for (const contact of selectedContacts.value) {
        const inviteeDid = await publicKeyToDidKeyAsync(contact.publicKey)
        const claims = await identityStore.getClaimsAsync(contact.id)
        const endpointIds = claims
          .filter(c => c.type === 'endpointId' || c.type.startsWith('device:'))
          .map(c => c.value)

        log.info(`Processing contact "${contact.label}" (did: ${inviteeDid.slice(0, 24)}..., ${endpointIds.length} endpoint(s))`)

        let serverInviteId: string | undefined

        // 1. Server invite if available
        if (space?.serverUrl) {
          try {
            const result = await spacesStore.inviteMemberAsync(
              props.serverUrl,
              props.spaceId,
              inviteeDid,
              selectedCapabilities.value[0]!,
              props.identityId,
              includeHistory.value,
            )
            serverInviteId = result.inviteId
            log.info(`Server invite created: ${result.inviteId}`)
          } catch (error) {
            log.warn(`Server invite failed for "${contact.label}", continuing with QUIC`, error)
          }
        }

        // 2. Always queue QUIC PushInvite (DB-based, works for both local and online spaces)
        if (endpointIds.length > 0) {
          try {
            await spacesStore.queueQuicInviteAsync({
              spaceId: props.spaceId,
              tokenId: serverInviteId,
              contactDid: inviteeDid,
              contactEndpointIds: endpointIds,
              capabilities: selectedCapabilities.value,
              includeHistory: includeHistory.value,
              expiresInSeconds: selectedExpiry.value!.value,
            })
            log.info(`QUIC invite queued for "${contact.label}" → ${endpointIds.length} endpoint(s)`)
          } catch (error) {
            // If server invite succeeded, QUIC failure is not fatal
            if (!serverInviteId) throw error
            log.warn(`QUIC invite failed for "${contact.label}", server invite was sent`, error)
          }
        } else {
          log.warn(`No endpoints for "${contact.label}", QUIC invite skipped`)
        }
      }
      log.info(`All contact invites processed for space ${props.spaceId}`)
      add({ title: t('success.invited'), color: 'success' })
      open.value = false
    } else if (space?.type === SpaceType.LOCAL) {
      // Local link/QR invite
      log.info(`Creating local invite link for space ${props.spaceId} (maxUses: ${maxUses.value})`)
      const { invoke } = await import('@tauri-apps/api/core')
      const tokenId = await invoke<string>('local_delivery_create_invite', {
        spaceId: props.spaceId,
        targetDid: null,
        capability: selectedCapabilities.value[0],
        maxUses: maxUses.value,
        expiresInSeconds: selectedExpiry.value!.value,
        includeHistory: includeHistory.value,
      })

      generatedLink.value = buildLocalInviteLink({
        spaceId: props.spaceId,
        tokenId,
        spaceEndpoints: selectedSpaceEndpoints.value,
      })
      generatedExpiresAt.value = new Date(Date.now() + selectedExpiry.value!.value * 1000).toISOString()
      log.info(`Local invite link created (token: ${tokenId}, endpoints: ${selectedSpaceEndpoints.value.length})`)

      await nextTick()
      if (qrCanvas.value) {
        await QRCode.toCanvas(qrCanvas.value, generatedLink.value, {
          width: 200,
          margin: 1,
          color: { dark: '#000000', light: '#ffffff' },
        })
      }
      add({ title: t('success.linkCreated'), color: 'success' })
    } else {
      // Online space: invite link
      log.info(`Creating online invite token for space ${props.spaceId} (maxUses: ${maxUses.value})`)
      const result = await spacesStore.createInviteTokenAsync(
        props.serverUrl,
        props.spaceId,
        {
          capability: selectedCapabilities.value[0],
          maxUses: maxUses.value,
          expiresInSeconds: selectedExpiry.value!.value,
          label: inviteLabel.value || undefined,
        },
      )
      generatedLink.value = spacesStore.buildInviteLink(props.serverUrl, props.spaceId, result.tokenId)
      generatedExpiresAt.value = result.expiresAt
      log.info(`Online invite link created (token: ${result.tokenId}, expires: ${result.expiresAt})`)

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
    log.error(`Invite failed (mode: ${props.mode}, space: ${props.spaceId})`, error)
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
  description:
    contact: Lade Kontakte direkt in diesen Space ein
    link: Erstelle einen Link oder QR-Code zum Teilen
  form:
    selectContacts: Kontakte auswählen
    noContacts: Keine Kontakte vorhanden
    manageContacts: Kontakte verwalten
    capabilityLabel: Berechtigungen
    deadlineLabel: Annahmefrist
    deadlineHint: Die Einladung verfällt, wenn sie nicht innerhalb dieser Zeit angenommen wird.
    label: Bezeichnung
    labelPlaceholder: z.B. Einladung für Max
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
    1d: 1 Tag
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
  description:
    contact: Directly invite contacts to this space
    link: Create a link or QR code to share
  form:
    selectContacts: Select contacts
    noContacts: No contacts found
    manageContacts: Manage contacts
    capabilityLabel: Permissions
    deadlineLabel: Acceptance deadline
    deadlineHint: The invitation expires if not accepted within this time.
    label: Label
    labelPlaceholder: e.g. Invite for Max
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
    1d: 1 day
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
