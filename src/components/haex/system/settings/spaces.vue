<template>
  <HaexSystemSettingsLayout :title="t('title')" :description="t('description')">
    <template #actions>
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

      <!-- Pending Invites Section -->
      <UCollapsible v-model:open="showInvitesSection" :unmount-on-hide="false">
        <div class="flex items-center gap-1.5 py-2 mb-3 text-sm font-medium hover:text-primary transition-colors cursor-pointer">
          <UIcon
            name="i-lucide-chevron-right"
            class="w-4 h-4 transition-transform duration-200"
            :class="{ 'rotate-90': showInvitesSection }"
          />
          <UIcon name="i-lucide-mail" class="w-4 h-4" />
          <span>{{ t('invites.title') }}</span>
        </div>
        <template #content>
          <div class="mb-4">
            <PendingInvites />
          </div>
        </template>
      </UCollapsible>

      <USeparator class="mb-4" />

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

      <!-- Pending Spaces (from push invites) -->
      <div v-if="pendingSpaces.length" class="space-y-3 mb-4">
        <p class="text-sm font-medium text-muted">
          {{ t('list.pending') }}
        </p>
        <div
          v-for="space in pendingSpaces"
          :key="space.id"
          class="p-3 rounded-lg border border-dashed border-primary/30 bg-primary/5"
        >
          <div class="flex items-center justify-between">
            <div class="flex items-center gap-2 min-w-0">
              <p class="font-medium text-sm truncate">{{ space.name }}</p>
              <UBadge :label="space.type" color="info" variant="subtle" size="sm" />
            </div>
            <UButton
              :label="t('list.viewInvite')"
              color="primary"
              variant="soft"
              size="sm"
              @click="showInvitesSection = true"
            />
          </div>
        </div>
      </div>

      <USeparator v-if="pendingSpaces.length && activeSpaces.length" class="mb-4" />

      <!-- Active Spaces list -->
      <div
        v-else-if="activeSpaces.length"
        class="space-y-3"
      >
        <SpaceListItem
          v-for="space in activeSpaces"
          :key="space.id"
          :space="space"
          @edit="openEditDialog"
          @invite-contact="openInviteDialog($event, 'contact')"
          @invite-link="openInviteDialog($event, 'link')"
          @delete="prepareDeleteSpace"
          @leave="prepareLeaveSpace"
        />
      </div>

      <!-- Empty state -->
      <HaexSystemSettingsLayoutEmpty
        v-else-if="!pendingSpaces.length"
        :message="t('list.empty')"
        icon="i-lucide-layout-grid"
      />
    <!-- Create Space Dialog -->
    <UiDrawerModal
      v-model:open="showCreateDialog"
      :title="t('create.title')"
      :description="t('create.description')"
    >
      <template #content>
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
      <template #content>
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

    <!-- Edit Space Dialog -->
    <UiDrawerModal
      v-model:open="showEditDialog"
      :title="t('edit.title')"
    >
      <template #content>
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

    <!-- Invite Member Dialog -->
    <SpaceInviteDialog
      v-model:open="showInviteDialog"
      :space-id="inviteSpaceId"
      :server-url="inviteServerUrl"
      :identity-id="inviteIdentityId"
      :mode="inviteMode"
    />

    <!-- Delete Space Confirmation -->
    <UiDialogConfirm
      v-model:open="showDeleteConfirm"
      :title="t('delete.title')"
      :description="t('delete.description')"
      @confirm="onConfirmDeleteAsync"
    />

    <!-- Leave Space Confirmation -->
    <UiDialogConfirm
      v-model:open="showLeaveConfirm"
      :title="t('leave.title')"
      :description="t('leave.description')"
      @confirm="onConfirmLeaveAsync"
    />
  </HaexSystemSettingsLayout>
</template>

<script setup lang="ts">
import { SettingsCategory } from '~/config/settingsCategories'
import type { SpaceWithType } from '@/stores/spaces'
import SpaceListItem from './spaces/SpaceListItem.vue'
import SpaceInviteDialog from './spaces/SpaceInviteDialog.vue'
import PendingInvites from './spaces/PendingInvites.vue'
import { parseInviteTokenLink, parseLocalInviteLink } from '~/utils/inviteLink'
import { invoke } from '@tauri-apps/api/core'
import { SpaceType, type SpaceType as SpaceTypeValue } from '~/database/constants'

const props = defineProps<{
  inviteLink?: string
}>()

const { t } = useI18n()
const { add } = useToast()

const spacesStore = useSpacesStore()
const syncBackendsStore = useSyncBackendsStore()
const windowManager = useWindowManagerStore()

const { activeSpaces, pendingSpaces, spaces } = storeToRefs(spacesStore)
const { backends: syncBackends } = storeToRefs(syncBackendsStore)

// Loading states
const isLoadingSpaces = ref(false)
const isCreating = ref(false)
const isJoining = ref(false)

// Dialog visibility
const showCreateDialog = ref(false)
const showJoinDialog = ref(false)
const showInviteDialog = ref(false)
const showDeleteConfirm = ref(false)
const showLeaveConfirm = ref(false)
const showInvitesSection = ref(false)

// Create form
const createForm = reactive({
  name: '',
  type: SpaceType.LOCAL as SpaceTypeValue,
  serverUrl: undefined as { label: string; value: string } | undefined,
  identityId: undefined as string | undefined,
})

// Identity store
const identityStore = useIdentityStore()

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

    // Name changed?
    if (newName !== space.name) {
      await spacesStore.updateSpaceNameAsync(space.id, newName)
    }

    // Server changed?
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

// Server URL options from existing sync backends (with local-only default)
const serverUrlOptions = computed(() => {
  const options = [{ label: t('create.localOnly'), value: '' }]
  const urls = new Set<string>()
  for (const backend of syncBackends.value) {
    if (backend.homeServerUrl) {
      urls.add(backend.homeServerUrl)
    }
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

// Load spaces on mount
onMounted(async () => {
  await loadSpacesAsync()

  // Auto-open join dialog if launched with an invite link
  if (props.inviteLink) {
    joinInviteLink.value = props.inviteLink
    showJoinDialog.value = true
  }
})

const loadSpacesAsync = async () => {
  isLoadingSpaces.value = true
  try {
    // Ensure default space is loaded
    await spacesStore.ensureDefaultSpaceAsync()

    // Load remote spaces from all backends
    for (const backend of syncBackends.value) {
      if (backend.homeServerUrl) {
        await spacesStore.listSpacesAsync(backend.homeServerUrl, backend.identityId)
      }
    }
  } catch (error) {
    console.error('Failed to load spaces:', error)
  } finally {
    isLoadingSpaces.value = false
  }
}

// Create space
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
      // Online space — requires server + identity
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

      // Open invite dialog for the newly created space
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

// Join space via invite token link
const onJoinSpaceAsync = async () => {
  if (!joinInviteLink.value) return

  isJoining.value = true
  try {
    // Check if it's a local invite first
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

      // Try each endpoint until one works
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

    // Use first available identity
    await identityStore.loadIdentitiesAsync()
    const identityId = identityStore.ownIdentities[0]?.id
    if (!identityId) {
      add({ title: t('errors.noIdentity', 'No identity available. Create one first.'), color: 'error' })
      return
    }

    await spacesStore.claimInviteTokenAsync(
      tokenLink.serverUrl,
      tokenLink.spaceId,
      tokenLink.tokenId,
      identityId,
    )

    // Create a sync backend for this space
    await syncBackendsStore.addBackendAsync({
      name: `Space ${tokenLink.spaceId.slice(0, 8)}`,
      homeServerUrl: tokenLink.serverUrl,
      spaceId: tokenLink.spaceId,
      identityId,
      enabled: true,
    })

    add({
      title: t('success.joined'),
      color: 'success',
    })

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

// Find the identity linked to a space via its sync backend
const getIdentityForSpace = (spaceServerUrl: string): string | undefined => {
  const backend = syncBackends.value.find(b => b.homeServerUrl === spaceServerUrl)
  return backend?.identityId ?? undefined
}

// Open invite dialog
const openInviteDialog = (space: SpaceWithType, mode: 'contact' | 'link' = 'contact') => {
  inviteSpaceId.value = space.id
  inviteServerUrl.value = space.serverUrl
  inviteIdentityId.value = getIdentityForSpace(space.serverUrl) ?? ''
  inviteMode.value = mode
  showInviteDialog.value = true
}

// Prepare delete/leave
const prepareDeleteSpace = (space: SpaceWithType) => {
  targetSpace.value = space
  showDeleteConfirm.value = true
}

const prepareLeaveSpace = (space: SpaceWithType) => {
  targetSpace.value = space
  showLeaveConfirm.value = true
}

// Confirm delete
const onConfirmDeleteAsync = async () => {
  if (!targetSpace.value) return

  try {
    await spacesStore.deleteSpaceAsync(targetSpace.value.serverUrl, targetSpace.value.id)

    add({
      title: t('success.deleted'),
      color: 'success',
    })

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

// Confirm leave
const onConfirmLeaveAsync = async () => {
  if (!targetSpace.value) return

  try {
    const identityId = getIdentityForSpace(targetSpace.value.serverUrl)
    if (!identityId) {
      add({ title: t('errors.noIdentity', 'No identity linked to this space.'), color: 'error' })
      return
    }

    await spacesStore.leaveSpaceAsync(targetSpace.value.serverUrl, targetSpace.value.id, identityId)

    add({
      title: t('success.left'),
      color: 'success',
    })

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
  invites:
    title: Einladungen
  list:
    title: Deine Spaces
    description: Geteilte Spaces für die Zusammenarbeit mit anderen
    empty: Keine Spaces vorhanden
    pending: Ausstehende Einladungen
    viewInvite: Einladung ansehen
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
  errors:
    updateFailed: Space konnte nicht aktualisiert werden
    createFailed: Space konnte nicht erstellt werden
    joinFailed: Beitritt fehlgeschlagen
    deleteFailed: Löschen fehlgeschlagen
    leaveFailed: Verlassen fehlgeschlagen
    invalidInviteLink: Ungültiger Einladungslink
    invalidInvite: Unvollständige Einladung
    noServerUrl: Server-URL für diesen Space nicht gefunden
en:
  title: Spaces
  description: Create, manage and join shared spaces
  invites:
    title: Invitations
  list:
    title: Your Spaces
    description: Shared spaces for collaboration with others
    empty: No spaces found
    pending: Pending Invitations
    viewInvite: View Invitation
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
  errors:
    createFailed: Failed to create space
    updateFailed: Failed to update space
    joinFailed: Failed to join space
    deleteFailed: Failed to delete space
    leaveFailed: Failed to leave space
    invalidInviteLink: Invalid invite link
    invalidInvite: Invalid or incomplete invitation
    noServerUrl: Server URL for this space not found
</i18n>
