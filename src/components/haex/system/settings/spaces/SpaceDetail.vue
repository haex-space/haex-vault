<template>
  <HaexSystemSettingsLayout
    :title="space?.name ?? ''"
    show-back
    @back="$emit('back')"
  >
    <template #description>
      <div class="flex items-center gap-2">
        <UBadge
          v-if="space?.serverUrl"
          color="info"
          variant="subtle"
          size="sm"
          icon="i-lucide-cloud"
        >
          {{ backendName }}
        </UBadge>
        <UBadge
          v-else
          color="neutral"
          variant="subtle"
          size="sm"
          icon="i-lucide-hard-drive"
        >
          {{ t('type.local') }}
        </UBadge>
        <UBadge
          :color="permissionBadgeColor"
          variant="subtle"
          size="sm"
        >
          {{ permissionLabel }}
        </UBadge>
      </div>
    </template>

    <template #actions>
      <UDropdownMenu
        v-if="isAdmin || canInvite"
        :items="inviteMenuItems"
      >
        <UiButton
          color="primary"
          icon="i-lucide-user-plus"
        >
          <span class="hidden @sm:inline">{{ t('invite.label') }}</span>
        </UiButton>
      </UDropdownMenu>
      <UiButton
        v-if="isAdmin || canInvite"
        color="neutral"
        variant="outline"
        icon="i-lucide-users"
        @click="showMembersDrawer = true"
      >
        <span class="hidden @sm:inline">{{ t('actions.members') }}</span>
      </UiButton>
    </template>

    <!-- Loading -->
    <div
      v-if="isLoading"
      class="flex items-center justify-center py-8"
    >
      <UIcon
        name="i-lucide-loader-2"
        class="w-5 h-5 animate-spin text-primary"
      />
    </div>

    <!-- Linked Items -->
    <SpaceLinkedItems
      v-else
      :groups="groups"
      :is-loading="isLoading"
      :can-edit="isAdmin || canWrite"
      @open-group="onOpenGroup"
    />

    <!-- Members Drawer -->
    <UiDrawerModal
      v-model:open="showMembersDrawer"
      :title="t('members.title')"
      :description="t('members.description')"
    >
      <template #body>
        <!-- Pending invite tokens -->
        <div v-if="pendingTokens.length" class="space-y-2 mb-4">
          <p class="text-xs font-medium text-muted uppercase tracking-wide">
            {{ t('members.pendingInvites') }}
          </p>
          <div
            v-for="token in pendingTokens"
            :key="token.id"
            class="flex items-center justify-between gap-2 p-2 rounded-md bg-warning-50 dark:bg-warning-950/30"
          >
            <div class="min-w-0">
              <p class="text-sm truncate">
                {{ token.targetDid ? token.targetDid.slice(0, 24) + '…' : t('members.openInvite') }}
              </p>
              <p class="text-xs text-muted">
                {{ formatCapabilities(token.capabilities) }}
                · {{ t('members.uses', { current: token.currentUses, max: token.maxUses }) }}
              </p>
            </div>
            <div class="flex gap-1">
              <UiButton
                v-if="token.targetDid"
                color="primary"
                variant="ghost"
                icon="i-lucide-send"
                :title="t('members.resend')"
                :loading="resendingTokenId === token.id"
                @click="onResendInviteAsync(token)"
              />
              <UiButton
                v-if="isAdmin"
                color="error"
                variant="ghost"
                icon="i-lucide-trash-2"
                :title="t('members.revokeInvite')"
                @click="onRevokeTokenAsync(token.id)"
              />
            </div>
          </div>
        </div>

        <!-- Members (from space devices) -->
        <div class="space-y-2">
          <p class="text-xs font-medium text-muted uppercase tracking-wide">
            {{ t('members.active') }}
          </p>
          <div
            v-for="member in members"
            :key="member.deviceEndpointId"
            class="flex items-center justify-between gap-2 p-2 rounded-md bg-gray-50 dark:bg-gray-800/50"
          >
            <div class="flex items-center gap-2 min-w-0">
              <UiAvatar
                :src="member.avatar"
                :seed="member.deviceEndpointId"
                size="xs"
              />
              <div class="min-w-0">
                <p class="text-sm font-medium truncate">{{ member.deviceName }}</p>
                <p class="text-xs text-muted truncate">{{ member.deviceEndpointId.slice(0, 16) }}…</p>
              </div>
            </div>
            <UiButton
              v-if="isAdmin && !isOwnDevice(member.deviceEndpointId)"
              color="error"
              variant="ghost"
              icon="i-lucide-user-minus"              :title="t('members.remove')"
              @click="onRemoveMemberAsync(member)"
            />
          </div>

          <p
            v-if="members.length === 0"
            class="text-xs text-muted text-center py-3"
          >
            {{ t('members.empty') }}
          </p>
        </div>
      </template>
    </UiDrawerModal>
  </HaexSystemSettingsLayout>
</template>

<script setup lang="ts">
import { eq } from 'drizzle-orm'
import type { SpaceWithType } from '@/stores/spaces'
import type { SelectHaexSpaceDevices, SelectHaexInviteTokens } from '~/database/schemas'
import type { SpaceLinkedItemGroup } from '~/composables/useSpaceLinkedItems'
import { haexSpaceDevices, haexInviteTokens } from '~/database/schemas'
import { SettingsCategory } from '~/config/settingsCategories'
import SpaceLinkedItems from './SpaceLinkedItems.vue'

const props = defineProps<{
  spaceId: string
}>()

const emit = defineEmits<{
  back: []
  'invite-contact': [space: SpaceWithType]
  'invite-link': [space: SpaceWithType]
}>()

const { t } = useI18n()
const { add } = useToast()

const spacesStore = useSpacesStore()
const peerStorageStore = usePeerStorageStore()
const { getBackendNameByUrl } = useSyncBackendsStore()
const { currentVault } = storeToRefs(useVaultStore())

const getDb = () => currentVault.value?.drizzle

const showMembersDrawer = ref(false)
const capabilities = ref<string[]>([])
const members = ref<SelectHaexSpaceDevices[]>([])
const pendingTokens = ref<SelectHaexInviteTokens[]>([])

const space = computed(() => spacesStore.spaces.find(s => s.id === props.spaceId))
const backendName = computed(() => space.value ? getBackendNameByUrl(space.value.serverUrl) : '')
const isAdmin = computed(() => capabilities.value.includes('space/admin'))
const canInvite = computed(() => capabilities.value.includes('space/invite'))
const canWrite = computed(() => capabilities.value.includes('space/write'))

const permissionLabel = computed(() => {
  if (capabilities.value.includes('space/admin')) return 'Admin'
  if (capabilities.value.includes('space/invite')) return 'Invite'
  if (capabilities.value.includes('space/write')) return 'Write'
  return 'Read'
})

const permissionBadgeColor = computed(() => {
  if (capabilities.value.includes('space/admin')) return 'primary' as const
  if (capabilities.value.includes('space/write')) return 'info' as const
  return 'neutral' as const
})

const inviteMenuItems = computed(() => [
  [{
    label: t('invite.contact'),
    icon: 'i-lucide-user-plus',
    onSelect: () => space.value && emit('invite-contact', space.value),
  },
  {
    label: t('invite.link'),
    icon: 'i-lucide-link',
    onSelect: () => space.value && emit('invite-link', space.value),
  }],
])

const windowManager = useWindowManagerStore()

const onOpenGroup = (group: SpaceLinkedItemGroup) => {
  if (group.type === 'p2p-shares') {
    windowManager.openWindowAsync({
      type: 'system',
      sourceId: 'settings',
      params: { category: SettingsCategory.PeerStorage },
    })
  } else if (group.type === 'extension' && group.extensionId) {
    windowManager.openWindowAsync({
      type: 'extension',
      sourceId: group.extensionId,
    })
  }
}

const isOwnDevice = (endpointId: string) => endpointId === peerStorageStore.nodeId

// Linked items
const { groups, isLoading, loadAsync } = useSpaceLinkedItems(() => props.spaceId)

const loadMembersAsync = async () => {
  const db = getDb()
  if (!db) return

  members.value = await db.select().from(haexSpaceDevices)
    .where(eq(haexSpaceDevices.spaceId, props.spaceId))

  const allTokens = await db.select().from(haexInviteTokens)
    .where(eq(haexInviteTokens.spaceId, props.spaceId))

  pendingTokens.value = allTokens.filter(t =>
    t.currentUses < t.maxUses && (!t.expiresAt || new Date(t.expiresAt).getTime() > Date.now()),
  )
}

const formatCapabilities = (capabilities: string | null): string => {
  if (!capabilities) return ''
  try {
    const parsed = JSON.parse(capabilities) as string[]
    return parsed.map(c => c.replace('space/', '')).join(', ')
  } catch {
    return capabilities
  }
}

const onRemoveMemberAsync = async (member: SelectHaexSpaceDevices) => {
  try {
    if (member.identityId) {
      await spacesStore.removeIdentityFromSpaceAsync(props.spaceId, member.identityId)
    } else {
      const db = getDb()
      if (db) {
        await db.delete(haexSpaceDevices).where(eq(haexSpaceDevices.id, member.id))
      }
    }
    add({ title: t('members.removed'), color: 'success' })
    await loadMembersAsync()
  } catch (error) {
    add({
      title: t('members.removeFailed'),
      description: error instanceof Error ? error.message : undefined,
      color: 'error',
    })
  }
}

const resendingTokenId = ref<string | null>(null)

const onResendInviteAsync = async (token: SelectHaexInviteTokens) => {
  if (!token.targetDid) return

  resendingTokenId.value = token.id
  try {
    const identityStore = useIdentityStore()
    await identityStore.loadIdentitiesAsync()

    // Find the contact by DID to get their endpoint IDs
    const contact = identityStore.contacts.find(c => c.did === token.targetDid)
    if (!contact) {
      add({ title: t('members.resendNoContact'), color: 'error' })
      return
    }

    const claims = await identityStore.getClaimsAsync(contact.id)
    const endpointIds = claims
      .filter(c => c.type === 'endpointId' || c.type.startsWith('device:'))
      .map(c => c.value)

    if (endpointIds.length === 0) {
      add({ title: t('members.resendNoEndpoint'), color: 'error' })
      return
    }

    const { useInviteOutbox } = await import('@/composables/useInviteOutbox')
    const { createOutboxEntryAsync } = useInviteOutbox()

    const expiresAt = token.expiresAt || new Date(Date.now() + 7 * 24 * 60 * 60 * 1000).toISOString()

    for (const endpointId of endpointIds) {
      await createOutboxEntryAsync({
        spaceId: token.spaceId,
        tokenId: token.id,
        targetDid: token.targetDid,
        targetEndpointId: endpointId,
        expiresAt,
      })
    }

    add({ title: t('members.resendSuccess'), color: 'success' })
  } catch (error) {
    add({
      title: t('members.resendFailed'),
      description: error instanceof Error ? error.message : undefined,
      color: 'error',
    })
  } finally {
    resendingTokenId.value = null
  }
}

const onRevokeTokenAsync = async (tokenId: string) => {
  try {
    const db = getDb()
    if (db) {
      await db.delete(haexInviteTokens).where(eq(haexInviteTokens.id, tokenId))
    }
    add({ title: t('members.tokenRevoked'), color: 'success' })
    await loadMembersAsync()
  } catch (error) {
    add({
      title: t('members.revokeFailed'),
      description: error instanceof Error ? error.message : undefined,
      color: 'error',
    })
  }
}

onMounted(async () => {
  capabilities.value = await spacesStore.getCapabilitiesForSpaceAsync(props.spaceId)
  await loadAsync()
  await loadMembersAsync()
})
</script>

<i18n lang="yaml">
de:
  type:
    local: Lokal
  actions:
    members: Mitglieder
  invite:
    label: Einladen
    contact: Kontakt einladen
    link: Einladungslink erstellen
  members:
    title: Mitglieder
    description: Mitglieder und ausstehende Einladungen verwalten
    active: Aktive Mitglieder
    pendingInvites: Ausstehende Einladungen
    openInvite: Offene Einladung
    uses: "{current}/{max} genutzt"
    empty: Keine Mitglieder
    remove: Entfernen
    removed: Mitglied entfernt
    removeFailed: Mitglied konnte nicht entfernt werden
    tokenRevoked: Einladung widerrufen
    revokeFailed: Einladung konnte nicht widerrufen werden
    revokeInvite: Einladung widerrufen
    resend: Erneut senden
    resendSuccess: Einladung erneut gesendet
    resendFailed: Erneutes Senden fehlgeschlagen
    resendNoContact: Kontakt nicht gefunden
    resendNoEndpoint: Kontakt hat keine bekannten Endpoints
en:
  type:
    local: Local
  actions:
    members: Members
  invite:
    label: Invite
    contact: Invite contact
    link: Create invite link
  members:
    title: Members
    description: Manage members and pending invitations
    active: Active members
    pendingInvites: Pending invitations
    openInvite: Open invitation
    uses: "{current}/{max} used"
    empty: No members
    remove: Remove
    removed: Member removed
    removeFailed: Failed to remove member
    tokenRevoked: Invitation revoked
    revokeFailed: Failed to revoke invitation
    revokeInvite: Revoke invitation
    resend: Resend
    resendSuccess: Invitation resent
    resendFailed: Failed to resend invitation
    resendNoContact: Contact not found
    resendNoEndpoint: Contact has no known endpoints
</i18n>
