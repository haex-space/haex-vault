<template>
  <HaexSystemSettingsLayout :title="t('title')" :description="t('description')">
    <!-- Spaces List -->
    <UCard>
      <template #header>
        <div class="flex flex-wrap items-center justify-between gap-2">
          <div>
            <h3 class="text-lg font-semibold">{{ t('list.title') }}</h3>
            <p class="text-sm text-gray-500 dark:text-gray-400 mt-1">
              {{ t('list.description') }}
            </p>
          </div>
          <div class="flex gap-2">
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
          </div>
        </div>
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

      <!-- Spaces list -->
      <div
        v-else-if="spaces.length"
        class="space-y-3"
      >
        <SpaceListItem
          v-for="space in spaces"
          :key="space.id"
          :space="space"
          @edit="openEditDialog"
          @invite="openInviteDialog"
          @delete="prepareDeleteSpace"
          @leave="prepareLeaveSpace"
        />
      </div>

      <!-- Empty state -->
      <div
        v-else
        class="text-center py-4 text-gray-500 dark:text-gray-400"
      >
        {{ t('list.empty') }}
      </div>
    </UCard>

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
        <div class="space-y-2">
          <div class="flex items-center gap-2">
            <USelectMenu
              v-model="createForm.serverUrl"
              :items="serverUrlOptions"
              :placeholder="t('create.serverLabel')"
              class="flex-1"
            />
            <UiButton
              icon="i-lucide-server"
              variant="ghost"
              color="neutral"
              size="md"
              @click="onNavigateToSync"
            />
          </div>
          <p v-if="!serverUrlOptions.length" class="text-xs text-muted">
            {{ t('create.noServersHint') }}
          </p>
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
            :disabled="!createForm.name || !createForm.serverUrl?.value"
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
          <USelectMenu
            v-model="editForm.serverUrl"
            :items="editServerOptions"
            :placeholder="t('edit.serverPlaceholder')"
            class="w-full"
          />
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
      :caller-role="inviteSpaceCallerRole"
      :identity-id="inviteIdentityId"
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
import type { DecryptedSpace, SpaceInvite, SpaceRole } from '@haex-space/vault-sdk'
import SpaceListItem from './spaces/SpaceListItem.vue'
import SpaceInviteDialog from './spaces/SpaceInviteDialog.vue'
import { decodeInviteLink } from '~/utils/inviteLink'

const props = defineProps<{
  inviteLink?: string
}>()

const { t } = useI18n()
const { add } = useToast()

const spacesStore = useSpacesStore()
const syncBackendsStore = useSyncBackendsStore()
const windowManager = useWindowManagerStore()

const { spaces } = storeToRefs(spacesStore)
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

// Create form
const createForm = reactive({
  name: '',
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
const inviteSpaceCallerRole = ref<SpaceRole>('member')
const inviteIdentityId = ref('')

// Edit dialog
const showEditDialog = ref(false)
const isSavingEdit = ref(false)
const editingSpace = ref<DecryptedSpace | null>(null)
const editForm = reactive({
  name: '',
  serverUrl: undefined as { label: string; value: string } | undefined,
})

const editServerOptions = computed(() => {
  const options = [{ label: t('edit.noServer'), value: '' }]
  for (const backend of syncBackends.value) {
    options.push({ label: backend.name, value: backend.serverUrl })
  }
  return options
})

const openEditDialog = (space: DecryptedSpace) => {
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
      const identityId = identityStore.identities[0]?.publicKey
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
const targetSpace = ref<DecryptedSpace | null>(null)

// Server URL options from existing sync backends
const serverUrlOptions = computed(() => {
  const urls = new Set<string>()
  for (const backend of syncBackends.value) {
    if (backend.serverUrl) {
      urls.add(backend.serverUrl)
    }
  }
  return [...urls].map(url => ({
    label: url,
    value: url,
  }))
})

const onNavigateToSync = () => {
  windowManager.openWindowAsync({
    type: 'system',
    sourceId: 'settings',
    params: { category: 'sync' },
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
    const urls = new Set<string>()
    for (const backend of syncBackends.value) {
      if (backend.serverUrl) {
        urls.add(backend.serverUrl)
      }
    }
    for (const url of urls) {
      await spacesStore.listSpacesAsync(url)
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
    const isLocal = !createForm.serverUrl?.value || createForm.serverUrl.value === 'local'

    if (isLocal) {
      // Local space — no server needed
      const createdSpace = await spacesStore.createLocalSpaceAsync(createForm.name)

      add({ title: t('success.created'), color: 'success' })
      showCreateDialog.value = false
      createForm.name = ''
      createForm.serverUrl = undefined
    } else {
      // Remote space — requires server + identity
      const serverUrl = createForm.serverUrl!.value

      let identityId = createForm.identityId
      if (!identityId) {
        await identityStore.loadIdentitiesAsync()
        identityId = identityStore.identities[0]?.publicKey
      }
      if (!identityId) {
        add({ title: t('errors.noIdentity', 'No identity available. Create one first.'), color: 'error' })
        return
      }

      const createdSpace = await spacesStore.createSpaceAsync(serverUrl, createForm.name, t('create.defaultSelfLabel'), identityId)

      add({ title: t('success.created'), color: 'success' })
      showCreateDialog.value = false
      createForm.name = ''
      createForm.serverUrl = undefined

      // Open invite dialog for the newly created space
      openInviteDialog({ ...createdSpace, name: createForm.name, role: 'admin' as SpaceRole, serverUrl, createdAt: new Date().toISOString() })
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

// Join space
const onJoinSpaceAsync = async () => {
  if (!joinInviteLink.value) return

  isJoining.value = true
  try {
    let invite: SpaceInvite
    try {
      invite = decodeInviteLink(joinInviteLink.value.trim())
    } catch {
      add({ title: t('errors.invalidInviteLink'), color: 'error' })
      return
    }
    if (!invite.spaceId || !invite.serverUrl || !invite.accessToken || !invite.encryptedSpaceKey) {
      add({ title: t('errors.invalidInvite'), color: 'error' })
      return
    }

    // Use first available identity (TODO: let user pick)
    await identityStore.loadIdentitiesAsync()
    const identityId = identityStore.identities[0]?.publicKey
    if (!identityId) {
      add({ title: t('errors.noIdentity', 'No identity available. Create one first.'), color: 'error' })
      return
    }

    const { spaceId } = await spacesStore.joinSpaceFromInviteAsync(invite, identityId)

    // Create a sync backend for this space with linked identity
    await syncBackendsStore.addBackendAsync({
      name: `Space ${spaceId.slice(0, 8)}`,
      serverUrl: invite.serverUrl,
      vaultId: invite.spaceId,
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
  const backend = syncBackends.value.find(b => b.serverUrl === spaceServerUrl)
  return backend?.identityId ?? undefined
}

// Open invite dialog
const openInviteDialog = (space: DecryptedSpace) => {
  inviteSpaceId.value = space.id
  inviteSpaceCallerRole.value = space.role
  inviteServerUrl.value = space.serverUrl
  inviteIdentityId.value = getIdentityForSpace(space.serverUrl) ?? ''
  showInviteDialog.value = true
}

// Prepare delete/leave
const prepareDeleteSpace = (space: DecryptedSpace) => {
  targetSpace.value = space
  showDeleteConfirm.value = true
}

const prepareLeaveSpace = (space: DecryptedSpace) => {
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
  list:
    title: Deine Spaces
    description: Geteilte Spaces für die Zusammenarbeit mit anderen
    empty: Keine Spaces vorhanden
  create:
    title: Space erstellen
    description: Erstelle einen neuen geteilten Space
    nameLabel: Name
    serverLabel: Server auswählen
    noServersHint: Kein Server konfiguriert. Klicke auf das Zahnrad, um einen hinzuzufügen.
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
  list:
    title: Your Spaces
    description: Shared spaces for collaboration with others
    empty: No spaces found
  create:
    title: Create Space
    description: Create a new shared space
    nameLabel: Name
    serverLabel: Select server
    noServersHint: No server configured. Click the gear icon to add one.
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
