<template>
  <div class="h-full">
    <!-- Detail view -->
    <SpaceDetail
      v-if="activeView === 'detail' && selectedSpaceId"
      :space-id="selectedSpaceId"
      @back="goBack"
      @invite-contact="openInviteDialog($event, 'contact')"
      @invite-link="openInviteDialog($event, 'link')"
    />

    <!-- Index view -->
    <HaexSystemSettingsLayout
      v-else
      :title="t('title')"
      :description="t('description')"
    >
    <template #actions>
      <!-- Invite Policy -->
      <div class="w-52">
        <UiSelectMenu
          :model-value="policyOption"
          :items="policyOptions"
          :label="t('policy.label')"
          :search-input="false"
          @update:model-value="onPolicyChangeAsync"
        />
      </div>
      <UButton
        color="neutral"
        variant="outline"
        icon="i-lucide-log-in"
        @click="showJoinDialog = true"
      >
        <span class="hidden @sm:inline">{{ t('actions.join') }}</span>
      </UButton>
      <UButton
        color="primary"
        icon="i-lucide-plus"
        @click="showCreateDialog = true"
      >
        <span class="hidden @sm:inline">{{ t('actions.create') }}</span>
      </UButton>
    </template>

      <!-- Loading -->
      <div
        v-if="isLoadingSpaces"
        class="flex items-center justify-center py-8"
      >
        <UIcon
          name="i-lucide-loader-2"
          class="w-5 h-5 animate-spin text-primary"
        />
      </div>

      <!-- Unified Space list -->
      <div
        v-else-if="spaceListEntries.length"
        class="space-y-3"
      >
        <SpaceListItem
          v-for="entry in spaceListEntries"
          :key="entry.space.id"
          :space="entry.space"
          :pending="entry.kind === 'pending'"
          :invite="entry.kind === 'pending' ? entry.invite : undefined"
          @select="openSpaceDetail"
          @accept="onAcceptInviteAsync(entry.kind === 'pending' ? entry.invite : undefined)"
          @decline="onDeclineInviteAsync(entry.kind === 'pending' ? entry.invite : undefined)"
          @edit="openEditDialog"
          @invite-contact="openInviteDialog($event, 'contact')"
          @invite-link="openInviteDialog($event, 'link')"
          @delete="prepareDeleteSpace"
          @leave="prepareLeaveSpace"
        />
      </div>

      <!-- Empty state -->
      <HaexSystemSettingsLayoutEmpty
        v-else
        :message="t('list.empty')"
        icon="i-lucide-layout-grid"
      />

    <!-- Create Space Dialog -->
    <UiDrawerModal
      v-model:open="showCreateDialog"
      :title="t('create.title')"
      :description="t('create.description')"
    >
      <template #body>
        <UiInput
          v-model="createForm.name"
          :label="t('create.nameLabel')"
          @keydown.enter.prevent="onCreateSpaceAsync"
        />

        <!-- Type selector -->
        <div class="space-y-1.5">
          <label class="text-sm font-medium">{{ t('create.typeLabel') }}</label>
          <div class="grid grid-cols-2 gap-2">
            <button
              class="flex flex-col items-center gap-1.5 p-3 rounded-lg border transition-colors"
              :class="createForm.type === SpaceType.LOCAL
                ? 'border-primary bg-primary/5 text-primary'
                : 'border-default hover:border-primary/50'"
              @click="createForm.type = SpaceType.LOCAL"
            >
              <UIcon name="i-lucide-hard-drive" class="w-5 h-5" />
              <span class="text-sm font-medium">{{ t('create.typeLocal') }}</span>
              <span class="text-xs text-muted text-center">{{ t('create.typeLocalHint') }}</span>
            </button>
            <button
              class="flex flex-col items-center gap-1.5 p-3 rounded-lg border transition-colors"
              :class="createForm.type === SpaceType.ONLINE
                ? 'border-primary bg-primary/5 text-primary'
                : 'border-default hover:border-primary/50'"
              @click="createForm.type = SpaceType.ONLINE"
            >
              <UIcon name="i-lucide-cloud" class="w-5 h-5" />
              <span class="text-sm font-medium">{{ t('create.typeOnline') }}</span>
              <span class="text-xs text-muted text-center">{{ t('create.typeOnlineHint') }}</span>
            </button>
          </div>
        </div>

        <!-- Server selector (only for online) -->
        <div v-if="createForm.type === SpaceType.ONLINE" class="flex items-center gap-2">
          <UiSelectMenu
            v-model="createForm.serverUrl"
            :items="serverUrlOptions"
            :label="t('create.serverLabel')"
            class="flex-1"
          />
          <UiButton
            icon="i-lucide-server"
            variant="outline"
            color="neutral"
            @click="onNavigateToSync"
          />
        </div>
      </template>
      <template #footer>
        <div class="flex justify-between gap-4">
          <UButton
            color="neutral"
            variant="outline"
            @click="showCreateDialog = false"
          >
            {{ t('actions.cancel') }}
          </UButton>
          <UiButton
            icon="i-lucide-plus"
            :loading="isCreating"
            :disabled="!createForm.name?.trim()"
            @click="onCreateSpaceAsync"
          >
            {{ t('actions.create') }}
          </UiButton>
        </div>
      </template>
    </UiDrawerModal>

    <!-- Join Space Dialog -->
    <UiDrawerModal
      v-model:open="showJoinDialog"
      :title="t('join.title')"
      :description="t('join.description')"
    >
      <template #body>
        <UiInput
          v-model="joinInviteLink"
          :label="t('join.inviteLabel')"
          :placeholder="t('join.invitePlaceholder')"
        />
      </template>
      <template #footer>
        <div class="flex justify-between gap-4">
          <UButton
            color="neutral"
            variant="outline"
            @click="showJoinDialog = false"
          >
            {{ t('actions.cancel') }}
          </UButton>
          <UiButton
            icon="i-lucide-log-in"
            :loading="isJoining"
            :disabled="!joinInviteLink"
            @click="onJoinSpaceAsync"
          >
            {{ t('actions.join') }}
          </UiButton>
        </div>
      </template>
    </UiDrawerModal>

    </HaexSystemSettingsLayout>

    <!-- Dialogs (rendered outside layout so they work in both index and detail views) -->
    <UiDrawerModal
      v-model:open="showEditDialog"
      :title="t('edit.title')"
    >
      <template #body>
        <UiInput
          v-model="editForm.name"
          :label="t('edit.nameLabel')"
          @keydown.enter.prevent="onSaveEditAsync"
        />
        <div class="space-y-2">
          <label class="text-sm font-medium">{{ t('edit.serverLabel') }}</label>
          <div class="flex items-center gap-2">
            <USelectMenu
              v-model="editForm.serverUrl"
              :items="editServerOptions"
              :placeholder="t('edit.serverPlaceholder')"
              :disabled="editingSpaceIsLocal"
              class="flex-1"
            />
            <UiButton
              icon="i-lucide-server"
              variant="outline"
              color="neutral"
              @click="onNavigateToSync"
            />
          </div>
        </div>
      </template>
      <template #footer>
        <div class="flex justify-between gap-4">
          <UButton
            color="neutral"
            variant="outline"
            @click="showEditDialog = false"
          >
            {{ t('actions.cancel') }}
          </UButton>
          <UiButton
            icon="i-lucide-save"
            :loading="isSavingEdit"
            :disabled="!editForm.name?.trim()"
            @click="onSaveEditAsync"
          >
            {{ t('actions.save') }}
          </UiButton>
        </div>
      </template>
    </UiDrawerModal>

    <SpaceInviteDialog
      v-model:open="showInviteDialog"
      :space-id="inviteSpaceId"
      :server-url="inviteServerUrl"
      :identity-id="inviteIdentityId"
      :mode="inviteMode"
    />

    <UiDialogConfirm
      v-model:open="showDeleteConfirm"
      :title="t('delete.title')"
      :description="t('delete.description')"
      @confirm="onConfirmDeleteAsync"
    />

    <UiDialogConfirm
      v-model:open="showLeaveConfirm"
      :title="t('leave.title')"
      :description="t('leave.description')"
      @confirm="onConfirmLeaveAsync"
    />
  </div>
</template>

<script setup lang="ts">
import { eq } from 'drizzle-orm'
import { listen } from '@tauri-apps/api/event'
import { invoke } from '@tauri-apps/api/core'
import { SettingsCategory } from '~/config/settingsCategories'
import type { SpaceWithType } from '@/stores/spaces'
import { haexPendingInvites, type SelectHaexPendingInvites } from '~/database/schemas'
import SpaceListItem from './spaces/SpaceListItem.vue'
import SpaceDetail from './spaces/SpaceDetail.vue'
import SpaceInviteDialog from './spaces/SpaceInviteDialog.vue'
import { parseInviteTokenLink, parseLocalInviteLink } from '~/utils/inviteLink'
import { SpaceType, SpaceStatus, type SpaceType as SpaceTypeValue } from '~/database/constants'
import { fetchWithDidAuth } from '@/utils/auth/didAuth'
import { useInvitePolicy } from '@/composables/useInvitePolicy'
import { useMlsDelivery } from '@/composables/useMlsDelivery'

type SpaceListEntry =
  | { kind: 'active'; space: SpaceWithType }
  | { kind: 'pending'; space: SpaceWithType; invite: SelectHaexPendingInvites }

const props = defineProps<{
  inviteLink?: string
}>()

const { t } = useI18n()
const { add } = useToast()

const tabId = inject<string>('haex-tab-id')!
const { activeView, navigationContext, navigateTo, goBack } = useDrillDownNavigation<'index' | 'detail'>('index', 'spaces', tabId)
const selectedSpaceId = computed(() => navigationContext.value.spaceId as string | null)

const openSpaceDetail = (space: SpaceWithType) => {
  navigateTo('detail', { spaceId: space.id })
}

const spacesStore = useSpacesStore()
const syncBackendsStore = useSyncBackendsStore()
const identityStore = useIdentityStore()
const windowManager = useWindowManagerStore()
const { currentVault } = storeToRefs(useVaultStore())

const { activeSpaces, spaces } = storeToRefs(spacesStore)
const { backends: syncBackends } = storeToRefs(syncBackendsStore)

const getDb = () => currentVault.value?.drizzle

// =========================================================================
// Pending invites (migrated from PendingInvites.vue)
// =========================================================================

const { setPolicy, getPolicy } = useInvitePolicy()

const pendingInvites = ref<SelectHaexPendingInvites[]>([])
const currentPolicy = ref<'all' | 'contacts_only' | 'nobody'>('contacts_only')

const policyOptions = computed(() => [
  { label: t('policy.all'), value: 'all' },
  { label: t('policy.contactsOnly'), value: 'contacts_only' },
  { label: t('policy.nobody'), value: 'nobody' },
])

const policyOption = computed(() =>
  policyOptions.value.find(o => o.value === currentPolicy.value),
)

const onPolicyChangeAsync = async (option: { label: string; value: string }) => {
  try {
    await setPolicy(option.value as 'all' | 'contacts_only' | 'nobody')
    currentPolicy.value = option.value as 'all' | 'contacts_only' | 'nobody'
  } catch (error) {
    console.error('Failed to update policy:', error)
    add({ title: t('errors.policyFailed'), color: 'error' })
  }
}

const loadInvitesAsync = async () => {
  const db = getDb()
  if (!db) return

  const rows = await db
    .select()
    .from(haexPendingInvites)
    .where(eq(haexPendingInvites.status, 'pending'))

  pendingInvites.value = rows
  currentPolicy.value = await getPolicy()
}

// =========================================================================
// Unified space list
// =========================================================================

const spaceListEntries = computed((): SpaceListEntry[] => {
  const entries: SpaceListEntry[] = []

  // Pending invites first — construct space from invite metadata
  // (no dummy entry in haex_spaces to avoid CRDT tombstone issues)
  for (const invite of pendingInvites.value) {
    const space: SpaceWithType = {
      id: invite.spaceId,
      name: invite.spaceName || invite.spaceId.slice(0, 8),
      type: (invite.spaceType as SpaceWithType['type']) || SpaceType.LOCAL,
      status: SpaceStatus.PENDING,
      serverUrl: invite.originUrl || '',
      createdAt: invite.createdAt || '',
    }
    entries.push({ kind: 'pending', space, invite })
  }

  // Active spaces
  for (const space of activeSpaces.value) {
    entries.push({ kind: 'active', space })
  }

  return entries
})

// =========================================================================
// Accept / Decline invite
// =========================================================================

const getServerUrlForSpace = (spaceId: string): string | undefined => {
  const backend = syncBackends.value.find(b => b.spaceId === spaceId)
  return backend?.homeServerUrl
}

const getIdentityAsync = async (): Promise<{ privateKey: string; did: string }> => {
  await identityStore.loadIdentitiesAsync()
  const identity = identityStore.ownIdentities[0]
  if (!identity?.privateKey) throw new Error('No identity available')
  return { privateKey: identity.privateKey, did: identity.did }
}

const onAcceptInviteAsync = async (invite?: SelectHaexPendingInvites) => {
  if (!invite) return

  try {
    // Determine the best acceptance path:
    // 1. Invite has QUIC endpoints → accept via QUIC ClaimInvite (invite was pushed via P2P)
    // 2. Invite has origin URL → accept via server (tokenId = server inviteId)
    const serverUrl = invite.originUrl || getServerUrlForSpace(invite.spaceId)
    const endpoints: string[] = invite.spaceEndpoints
      ? JSON.parse(invite.spaceEndpoints)
      : []

    if (endpoints.length > 0) {
      // QUIC invite — accept via ClaimInvite to one of the space endpoints
      // (acceptLocalInviteAsync creates the real space entry on success)
      await spacesStore.acceptLocalInviteAsync(invite)
    } else if (serverUrl && invite.tokenId) {
      // Online space without QUIC endpoints — accept via server
      const identity = await getIdentityAsync()
      const delivery = useMlsDelivery(serverUrl, invite.spaceId, {
        privateKey: identity.privateKey,
        did: identity.did,
      })
      await delivery.acceptInviteAsync(invite.tokenId)

      // Create the real space entry (no dummy space exists anymore)
      await spacesStore.persistSpaceAsync({
        id: invite.spaceId,
        name: invite.spaceName || invite.spaceId.slice(0, 8),
        type: (invite.spaceType as SpaceWithType['type']) || SpaceType.ONLINE,
        status: SpaceStatus.ACTIVE,
        serverUrl,
        createdAt: new Date().toISOString(),
      })
      await spacesStore.loadSpacesFromDbAsync()

      // Add self as space member
      const myIdentity = identityStore.ownIdentities[0]
      if (myIdentity) {
        await spacesStore.addMemberToSpaceAsync({
          spaceId: invite.spaceId,
          memberDid: myIdentity.did,
          label: myIdentity.label || myIdentity.did.slice(0, 16),
          role: 'read',
          avatar: myIdentity.avatar,
          avatarOptions: myIdentity.avatarOptions,
        })
      }
    } else {
      add({ title: t('errors.noServerUrl'), color: 'error' })
      return
    }

    // Mark invite as accepted
    const db = getDb()
    if (db) {
      await db.update(haexPendingInvites).set({
        status: 'accepted',
        respondedAt: new Date().toISOString(),
      }).where(eq(haexPendingInvites.id, invite.id))
    }

    add({ title: t('success.accepted'), color: 'success' })
    await loadInvitesAsync()
  } catch (error) {
    console.error('Failed to accept invite:', error)
    add({
      title: t('errors.acceptFailed'),
      description: error instanceof Error ? error.message : undefined,
      color: 'error',
    })
  }
}

const onDeclineInviteAsync = async (invite?: SelectHaexPendingInvites) => {
  if (!invite) return

  try {
    // If the invite has a server URL, also decline there (best-effort)
    const serverUrl = invite.originUrl || getServerUrlForSpace(invite.spaceId)

    if (serverUrl && invite.tokenId) {
      try {
        const identity = await getIdentityAsync()
        await fetchWithDidAuth(
          `${serverUrl}/spaces/${invite.spaceId}/invites/${invite.tokenId}/decline`,
          identity.privateKey,
          identity.did,
          'decline-invite',
          { method: 'POST', headers: { 'Content-Type': 'application/json' } },
        )
      } catch {
        // Server decline is best-effort — invite will expire on server
      }
    }

    // Mark invite as declined (CRDT delete is safe — haex_pending_invites rows
    // have unique UUIDs that don't collide with any row on the sender's device)
    const db = getDb()
    if (db) {
      await db.delete(haexPendingInvites).where(eq(haexPendingInvites.id, invite.id))
    }

    add({ title: t('success.declined'), color: 'success' })
    await loadInvitesAsync()
  } catch (error) {
    console.error('Failed to decline invite:', error)
    add({
      title: t('errors.declineFailed'),
      description: error instanceof Error ? error.message : undefined,
      color: 'error',
    })
  }
}

// =========================================================================
// Loading states & dialog visibility
// =========================================================================

const isLoadingSpaces = ref(false)
const isCreating = ref(false)
const isJoining = ref(false)

const showCreateDialog = ref(false)
const showJoinDialog = ref(false)
const showInviteDialog = ref(false)
const showDeleteConfirm = ref(false)
const showLeaveConfirm = ref(false)

// Create form
const createForm = reactive({
  name: '',
  type: SpaceType.LOCAL as SpaceTypeValue,
  serverUrl: undefined as { label: string; value: string } | undefined,
  identityId: undefined as string | undefined,
})

// Join form
const joinInviteLink = ref('')

// Invite dialog state
const inviteSpaceId = ref('')
const inviteServerUrl = ref('')
const inviteMode = ref<'contact' | 'link'>('contact')
const inviteIdentityId = ref('')

// Edit dialog
const showEditDialog = ref(false)
const isSavingEdit = ref(false)
const editingSpace = ref<SpaceWithType | null>(null)
const editForm = reactive({
  name: '',
  serverUrl: undefined as { label: string; value: string } | undefined,
})

const editingSpaceIsLocal = computed(() => {
  const space = spaces.value.find(s => s.id === editingSpace.value?.id)
  return space?.type === SpaceType.LOCAL
})

const editServerOptions = computed(() => {
  const options = [{ label: t('edit.noServer'), value: '' }]
  for (const backend of syncBackends.value) {
    options.push({ label: backend.name, value: backend.homeServerUrl })
  }
  return options
})

const openEditDialog = (space: SpaceWithType) => {
  editingSpace.value = space
  editForm.name = space.name
  editForm.serverUrl = space.serverUrl
    ? editServerOptions.value.find(o => o.value === space.serverUrl)
    : editServerOptions.value[0]
  showEditDialog.value = true
}

const onSaveEditAsync = async () => {
  if (!editingSpace.value || !editForm.name?.trim()) return

  isSavingEdit.value = true
  try {
    const space = editingSpace.value
    const newName = editForm.name.trim()
    const newServerUrl = editForm.serverUrl?.value || ''
    const oldServerUrl = space.serverUrl

    if (newName !== space.name) {
      await spacesStore.updateSpaceNameAsync(space.id, newName)
    }

    if (newServerUrl !== oldServerUrl) {
      const identityId = identityStore.ownIdentities[0]?.id
      if (!identityId && newServerUrl) {
        add({ title: t('errors.noIdentity'), color: 'error' })
        return
      }
      await spacesStore.migrateSpaceServerAsync(space.id, oldServerUrl, newServerUrl, identityId!)
    }

    add({ title: t('success.updated'), color: 'success' })
    showEditDialog.value = false
  } catch (error) {
    console.error('Failed to update space:', error)
    add({
      title: t('errors.updateFailed'),
      description: error instanceof Error ? error.message : 'Unknown error',
      color: 'error',
    })
  } finally {
    isSavingEdit.value = false
  }
}

// Delete/Leave target
const targetSpace = ref<SpaceWithType | null>(null)

// Server URL options
const serverUrlOptions = computed(() => {
  const options = [{ label: t('create.localOnly'), value: '' }]
  const urls = new Set<string>()
  for (const backend of syncBackends.value) {
    if (backend.homeServerUrl) urls.add(backend.homeServerUrl)
  }
  for (const url of urls) {
    options.push({ label: url, value: url })
  }
  return options
})

const onNavigateToSync = () => {
  showCreateDialog.value = false
  showEditDialog.value = false
  windowManager.openWindowAsync({
    type: 'system',
    sourceId: 'settings',
    params: { category: SettingsCategory.Sync },
  })
}

// =========================================================================
// Lifecycle
// =========================================================================

let unlistenPushInvite: (() => void) | null = null

onMounted(async () => {
  await loadSpacesAsync()

  // Auto-open join dialog if launched with an invite link
  if (props.inviteLink) {
    joinInviteLink.value = props.inviteLink
    showJoinDialog.value = true
  }

  // Listen for incoming P2P invites
  unlistenPushInvite = await listen('push-invite-received', async () => {
    await spacesStore.loadSpacesFromDbAsync()
    await loadInvitesAsync()
    add({ title: t('success.newInvite'), color: 'info' })
  })
})

onUnmounted(() => {
  unlistenPushInvite?.()
})

const loadSpacesAsync = async () => {
  isLoadingSpaces.value = true
  try {
    await spacesStore.ensureDefaultSpaceAsync()

    for (const backend of syncBackends.value) {
      if (backend.homeServerUrl) {
        await spacesStore.listSpacesAsync(backend.homeServerUrl, backend.identityId)
      }
    }

    await loadInvitesAsync()
  } catch (error) {
    console.error('Failed to load spaces:', error)
  } finally {
    isLoadingSpaces.value = false
  }
}

// =========================================================================
// Create / Join / Invite / Delete / Leave
// =========================================================================

const onCreateSpaceAsync = async () => {
  if (!createForm.name.trim()) return

  isCreating.value = true
  try {
    if (createForm.type === SpaceType.LOCAL) {
      await spacesStore.createLocalSpaceAsync(createForm.name)
      add({ title: t('success.created'), color: 'success' })
      showCreateDialog.value = false
      createForm.name = ''
    } else {
      const serverUrl = createForm.serverUrl?.value
      if (!serverUrl) {
        add({ title: t('errors.noServer'), color: 'error' })
        return
      }

      let identityId = createForm.identityId
      if (!identityId) {
        await identityStore.loadIdentitiesAsync()
        identityId = identityStore.ownIdentities[0]?.id
      }
      if (!identityId) {
        add({ title: t('errors.noIdentity'), color: 'error' })
        return
      }

      const createdSpace = await spacesStore.createSpaceAsync(serverUrl, createForm.name, t('create.defaultSelfLabel'), identityId)
      add({ title: t('success.created'), color: 'success' })
      showCreateDialog.value = false
      createForm.name = ''
      createForm.serverUrl = undefined

      openInviteDialog({ ...createdSpace, name: createForm.name, serverUrl, createdAt: new Date().toISOString() } as SpaceWithType)
    }
  } catch (error) {
    console.error('Failed to create space:', error)
    add({
      title: t('errors.createFailed'),
      description: error instanceof Error ? error.message : 'Unknown error',
      color: 'error',
    })
  } finally {
    isCreating.value = false
  }
}

const onJoinSpaceAsync = async () => {
  if (!joinInviteLink.value) return

  isJoining.value = true
  try {
    const localLink = parseLocalInviteLink(joinInviteLink.value.trim())
    if (localLink) {
      await identityStore.loadIdentitiesAsync()
      const identityId = identityStore.ownIdentities[0]?.id
      if (!identityId) {
        add({ title: t('errors.noIdentity'), color: 'error' })
        return
      }
      const identity = await identityStore.getIdentityByIdAsync(identityId)
      if (!identity) {
        add({ title: t('errors.noIdentity'), color: 'error' })
        return
      }

      let lastError: Error | null = null
      for (const endpointId of localLink.spaceEndpoints) {
        try {
          await invoke('local_delivery_claim_invite', {
            leaderEndpointId: endpointId,
            leaderRelayUrl: null,
            spaceId: localLink.spaceId,
            tokenId: localLink.tokenId,
            identityDid: identity.did,
            label: identity.label || null,
          })
          lastError = null
          break
        } catch (error) {
          lastError = error instanceof Error ? error : new Error(String(error))
        }
      }
      if (lastError) throw lastError

      add({ title: t('success.joined'), color: 'success' })
      showJoinDialog.value = false
      joinInviteLink.value = ''
      await spacesStore.loadSpacesFromDbAsync()
      return
    }

    const tokenLink = parseInviteTokenLink(joinInviteLink.value.trim())
    if (!tokenLink) {
      add({ title: t('errors.invalidInviteLink'), color: 'error' })
      return
    }

    await identityStore.loadIdentitiesAsync()
    const identityId = identityStore.ownIdentities[0]?.id
    if (!identityId) {
      add({ title: t('errors.noIdentity'), color: 'error' })
      return
    }

    await spacesStore.claimInviteTokenAsync(tokenLink.serverUrl, tokenLink.spaceId, tokenLink.tokenId, identityId)

    await syncBackendsStore.addBackendAsync({
      name: `Space ${tokenLink.spaceId.slice(0, 8)}`,
      homeServerUrl: tokenLink.serverUrl,
      spaceId: tokenLink.spaceId,
      identityId,
      enabled: true,
    })

    add({ title: t('success.joined'), color: 'success' })
    showJoinDialog.value = false
    joinInviteLink.value = ''
    await loadSpacesAsync()
  } catch (error) {
    console.error('Failed to join space:', error)
    add({
      title: t('errors.joinFailed'),
      description: error instanceof Error ? error.message : 'Unknown error',
      color: 'error',
    })
  } finally {
    isJoining.value = false
  }
}

const getIdentityForSpace = (spaceServerUrl: string): string | undefined => {
  const backend = syncBackends.value.find(b => b.homeServerUrl === spaceServerUrl)
  return backend?.identityId ?? undefined
}

const openInviteDialog = (space: SpaceWithType, mode: 'contact' | 'link' = 'contact') => {
  inviteSpaceId.value = space.id
  inviteServerUrl.value = space.serverUrl
  inviteIdentityId.value = getIdentityForSpace(space.serverUrl) ?? ''
  inviteMode.value = mode
  showInviteDialog.value = true
}

const prepareDeleteSpace = (space: SpaceWithType) => {
  targetSpace.value = space
  showDeleteConfirm.value = true
}

const prepareLeaveSpace = (space: SpaceWithType) => {
  targetSpace.value = space
  showLeaveConfirm.value = true
}

const onConfirmDeleteAsync = async () => {
  if (!targetSpace.value) return
  try {
    await spacesStore.deleteSpaceAsync(targetSpace.value.serverUrl, targetSpace.value.id)
    add({ title: t('success.deleted'), color: 'success' })
    showDeleteConfirm.value = false
    targetSpace.value = null
  } catch (error) {
    console.error('Failed to delete space:', error)
    add({
      title: t('errors.deleteFailed'),
      description: error instanceof Error ? error.message : 'Unknown error',
      color: 'error',
    })
  }
}

const onConfirmLeaveAsync = async () => {
  if (!targetSpace.value) return
  try {
    const identityId = getIdentityForSpace(targetSpace.value.serverUrl)
    if (!identityId) {
      add({ title: t('errors.noIdentity'), color: 'error' })
      return
    }
    await spacesStore.leaveSpaceAsync(targetSpace.value.serverUrl, targetSpace.value.id, identityId)
    add({ title: t('success.left'), color: 'success' })
    showLeaveConfirm.value = false
    targetSpace.value = null
  } catch (error) {
    console.error('Failed to leave space:', error)
    add({
      title: t('errors.leaveFailed'),
      description: error instanceof Error ? error.message : 'Unknown error',
      color: 'error',
    })
  }
}
</script>

<i18n lang="yaml">
de:
  title: Spaces
  description: Erstelle, verwalte und tritt geteilten Spaces bei
  policy:
    label: "Einladungen erlaubt von:"
    all: Alle
    contactsOnly: Nur Kontakte
    nobody: Niemand
  list:
    empty: Keine Spaces vorhanden
  create:
    title: Space erstellen
    description: Erstelle einen neuen geteilten Space
    nameLabel: Name
    typeLabel: Typ
    typeLocal: Lokal
    typeOnline: Online
    typeLocalHint: Daten bleiben auf deinen Geräten
    typeOnlineHint: Synchronisiert über einen Server
    serverLabel: Sync-Server
    localOnly: Lokal (ohne Server)
    defaultSelfLabel: Ich
  join:
    title: Space beitreten
    description: Tritt einem Space mit einem Einladungslink bei
    inviteLabel: Einladungslink
    invitePlaceholder: haexvault://invite/...
  edit:
    title: Space bearbeiten
    nameLabel: Name
    serverLabel: Sync-Server
    serverPlaceholder: Server auswählen
    noServer: Kein Server (lokal)
  delete:
    title: Space löschen
    description: Möchtest du diesen Space wirklich löschen? Alle Daten werden unwiderruflich entfernt.
  leave:
    title: Space verlassen
    description: Möchtest du diesen Space wirklich verlassen? Du kannst nur durch eine erneute Einladung wieder beitreten.
  actions:
    create: Erstellen
    join: Beitreten
    cancel: Abbrechen
    save: Speichern
  success:
    created: Space erstellt
    joined: Space beigetreten
    deleted: Space gelöscht
    updated: Space aktualisiert
    left: Space verlassen
    accepted: Einladung angenommen
    declined: Einladung abgelehnt
    newInvite: Neue Einladung erhalten
  errors:
    updateFailed: Space konnte nicht aktualisiert werden
    createFailed: Space konnte nicht erstellt werden
    joinFailed: Beitritt fehlgeschlagen
    deleteFailed: Löschen fehlgeschlagen
    leaveFailed: Verlassen fehlgeschlagen
    invalidInviteLink: Ungültiger Einladungslink
    noServerUrl: Server-URL für diesen Space nicht gefunden
    noIdentity: Keine Identität verfügbar
    noServer: Kein Server ausgewählt
    acceptFailed: Einladung konnte nicht angenommen werden
    declineFailed: Einladung konnte nicht abgelehnt werden
    policyFailed: Richtlinie konnte nicht aktualisiert werden
en:
  title: Spaces
  description: Create, manage and join shared spaces
  policy:
    label: "Invitations allowed from:"
    all: Everyone
    contactsOnly: Contacts only
    nobody: Nobody
  list:
    empty: No spaces found
  create:
    title: Create Space
    description: Create a new shared space
    nameLabel: Name
    typeLabel: Type
    typeLocal: Local
    typeOnline: Online
    typeLocalHint: Data stays on your devices
    typeOnlineHint: Synchronized via a server
    serverLabel: Sync Server
    localOnly: Local (no server)
    defaultSelfLabel: Me
  join:
    title: Join Space
    description: Join a space using an invite link
    inviteLabel: Invite link
    invitePlaceholder: haexvault://invite/...
  edit:
    title: Edit Space
    nameLabel: Name
    serverLabel: Sync Server
    serverPlaceholder: Select server
    noServer: No server (local)
  delete:
    title: Delete Space
    description: Do you really want to delete this space? All data will be permanently removed.
  leave:
    title: Leave Space
    description: Do you really want to leave this space? You can only rejoin with a new invitation.
  actions:
    create: Create
    join: Join
    cancel: Cancel
    save: Save
  success:
    created: Space created
    joined: Joined space
    deleted: Space deleted
    updated: Space updated
    left: Left space
    accepted: Invitation accepted
    declined: Invitation declined
    newInvite: New invitation received
  errors:
    createFailed: Failed to create space
    updateFailed: Failed to update space
    joinFailed: Failed to join space
    deleteFailed: Failed to delete space
    leaveFailed: Failed to leave space
    invalidInviteLink: Invalid invite link
    noServerUrl: Server URL for this space not found
    noIdentity: No identity available
    noServer: No server selected
    acceptFailed: Failed to accept invitation
    declineFailed: Failed to decline invitation
    policyFailed: Failed to update policy
</i18n>
