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

        <!-- Members -->
        <div class="space-y-2">
          <p class="text-xs font-medium text-muted uppercase tracking-wide">
            {{ t('members.active') }}
          </p>
          <div
            v-for="member in members"
            :key="member.id"
            class="flex items-center justify-between gap-2 p-2 rounded-md bg-gray-50 dark:bg-gray-800/50"
          >
            <div class="flex items-center gap-2 min-w-0">
              <UiAvatar
                :src="member.avatar"
                :seed="member.memberDid"
                size="xs"
              />
              <div v-if="editingMemberId === member.id" class="flex items-center gap-2 min-w-0">
                <UiInput
                  v-model="editLabel"
                  size="sm"
                  :placeholder="t('members.labelPlaceholder')"
                  @keyup.enter="onUpdateProfileAsync"
                  @keyup.escape="onCancelEditProfile"
                />
                <UiButton
                  color="primary"
                  variant="ghost"
                  icon="i-lucide-check"
                  :loading="savingProfile"
                  @click="onUpdateProfileAsync"
                />
                <UiButton
                  color="neutral"
                  variant="ghost"
                  icon="i-lucide-x"
                  @click="onCancelEditProfile"
                />
              </div>
              <div v-else class="min-w-0">
                <div class="flex items-center gap-1.5">
                  <p class="text-sm font-medium truncate">{{ member.label }}</p>
                  <UBadge
                    :color="member.role === 'admin' ? 'primary' : 'neutral'"
                    variant="subtle"
                    size="xs"
                  >
                    {{ member.role }}
                  </UBadge>
                </div>
                <p class="text-xs text-muted truncate">{{ member.memberDid.slice(0, 24) }}…</p>
              </div>
            </div>
            <div class="flex gap-1">
              <UiButton
                v-if="isOwnMember(member) && editingMemberId !== member.id"
                color="neutral"
                variant="ghost"
                icon="i-lucide-pencil"
                :title="t('members.editProfile')"
                @click="onStartEditProfile(member)"
              />
              <UiButton
                v-if="isAdmin && !isOwnMember(member)"
                color="error"
                variant="ghost"
                icon="i-lucide-user-minus"
                :title="t('members.remove')"
                @click="onRemoveMemberAsync(member)"
              />
            </div>
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
import type { SelectHaexSpaceMembers, SelectHaexInviteTokens } from '~/database/schemas'
import type { SpaceLinkedItemGroup } from '~/composables/useSpaceLinkedItems'
import { haexInviteTokens } from '~/database/schemas'
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
const identityStore = useIdentityStore()
const { getBackendNameByUrl } = useSyncBackendsStore()
const { currentVault } = storeToRefs(useVaultStore())

const getDb = () => currentVault.value?.drizzle

const showMembersDrawer = ref(false)
const capabilities = ref<string[]>([])
const members = ref<SelectHaexSpaceMembers[]>([])
const pendingTokens = ref<SelectHaexInviteTokens[]>([])

const editingMemberId = ref<string | null>(null)
const editLabel = ref('')
const savingProfile = ref(false)

const myDids = computed(() => identityStore.ownIdentities.map(i => i.did))
const isOwnMember = (member: SelectHaexSpaceMembers) => myDids.value.includes(member.memberDid)

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

// Linked items
const { groups, isLoading, loadAsync } = useSpaceLinkedItems(() => props.spaceId)

const loadMembersAsync = async () => {
  members.value = await spacesStore.getSpaceMembersAsync(props.spaceId)

  const db = getDb()
  if (!db) return

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

const onRemoveMemberAsync = async (member: SelectHaexSpaceMembers) => {
  try {
    await spacesStore.removeIdentityFromSpaceAsync(props.spaceId, member.memberDid)
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

const onStartEditProfile = (member: SelectHaexSpaceMembers) => {
  editingMemberId.value = member.id
  editLabel.value = member.label
}

const onCancelEditProfile = () => {
  editingMemberId.value = null
  editLabel.value = ''
}

const onUpdateProfileAsync = async () => {
  savingProfile.value = true
  try {
    await spacesStore.updateOwnSpaceProfileAsync(props.spaceId, {
      label: editLabel.value,
    })
    editingMemberId.value = null
    editLabel.value = ''
    await loadMembersAsync()
    add({ title: t('members.profileUpdated'), color: 'success' })
  } catch (error) {
    add({
      title: t('members.profileUpdateFailed'),
      description: error instanceof Error ? error.message : undefined,
      color: 'error',
    })
  } finally {
    savingProfile.value = false
  }
}

const resendingTokenId = ref<string | null>(null)

const onResendInviteAsync = async (token: SelectHaexInviteTokens) => {
  if (!token.targetDid) return

  resendingTokenId.value = token.id
  try {
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
    editProfile: Profil bearbeiten
    labelPlaceholder: Anzeigename
    profileUpdated: Profil aktualisiert
    profileUpdateFailed: Profil konnte nicht aktualisiert werden
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
    editProfile: Edit profile
    labelPlaceholder: Display name
    profileUpdated: Profile updated
    profileUpdateFailed: Failed to update profile
    tokenRevoked: Invitation revoked
    revokeFailed: Failed to revoke invitation
    revokeInvite: Revoke invitation
    resend: Resend
    resendSuccess: Invitation resent
    resendFailed: Failed to resend invitation
    resendNoContact: Contact not found
    resendNoEndpoint: Contact has no known endpoints
</i18n>
