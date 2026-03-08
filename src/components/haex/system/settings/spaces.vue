<template>
  <HaexSystemSettingsLayout :title="t('title')" :description="t('description')">
    <!-- Spaces List -->
    <UCard>
      <template #header>
        <div class="flex items-center justify-between">
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
        <div
          v-for="space in spaces"
          :key="space.id"
          class="flex flex-col gap-2 p-3 rounded-lg bg-gray-50 dark:bg-gray-800/50"
        >
          <div class="flex flex-col @xs:flex-row @xs:items-center @xs:justify-between gap-2">
            <div class="flex-1 min-w-0">
              <div class="flex items-center gap-2 flex-wrap">
                <p class="font-medium text-sm truncate">
                  Space {{ space.id.slice(0, 8) }}
                </p>
                <UBadge
                  :color="roleBadgeColor(space.role)"
                  variant="subtle"
                  size="xs"
                >
                  {{ t(`roles.${space.role}`) }}
                </UBadge>
              </div>
              <p class="text-xs text-gray-500 dark:text-gray-400 mt-1">
                {{ t('list.createdAt') }}: {{ formatDate(space.createdAt) }}
              </p>
            </div>
            <div class="flex gap-2 @xs:shrink-0">
              <UButton
                v-if="space.role === 'admin'"
                color="primary"
                variant="ghost"
                icon="i-lucide-user-plus"
                size="sm"
                :title="t('actions.invite')"
                @click="openInviteDialog(space)"
              />
              <UButton
                v-if="space.role === 'admin'"
                color="error"
                variant="ghost"
                icon="i-lucide-trash-2"
                size="sm"
                :title="t('actions.delete')"
                @click="prepareDeleteSpace(space)"
              />
              <UButton
                v-else
                color="warning"
                variant="ghost"
                icon="i-lucide-log-out"
                size="sm"
                :title="t('actions.leave')"
                @click="prepareLeaveSpace(space)"
              />
            </div>
          </div>
        </div>
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
        <USelectMenu
          v-model="createForm.serverUrl"
          :items="serverUrlOptions"
          :placeholder="t('create.serverLabel')"
          class="w-full"
        />
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
            :disabled="!createForm.name || !createForm.serverUrl"
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
        <UiTextarea
          v-model="joinInviteJson"
          :label="t('join.inviteLabel')"
          rows="8"
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
            :disabled="!joinInviteJson"
            @click="onJoinSpaceAsync"
          >
            {{ t('actions.join') }}
          </UiButton>
        </div>
      </template>
    </UiDrawerModal>

    <!-- Invite Member Dialog -->
    <UiDrawerModal
      v-model:open="showInviteDialog"
      :title="t('invite.title')"
      :description="t('invite.description')"
    >
      <template #content>
        <template v-if="!inviteResult">
          <UiInput
            v-model="inviteForm.userId"
            :label="t('invite.userIdLabel')"
            @keydown.enter.prevent="onInviteMemberAsync"
          />
          <USelectMenu
            v-model="inviteForm.role"
            :items="roleOptions"
            :placeholder="t('invite.roleLabel')"
            class="w-full"
          />
        </template>
        <template v-else>
          <p class="text-sm text-gray-500 dark:text-gray-400 mb-2">
            {{ t('invite.resultDescription') }}
          </p>
          <UiTextarea
            :model-value="inviteResult"
            read-only
            rows="10"
            :label="t('invite.resultLabel')"
          />
        </template>
      </template>
      <template #footer>
        <div class="flex justify-between gap-4">
          <UButton
            color="neutral"
            variant="outline"
            @click="closeInviteDialog"
          >
            {{ inviteResult ? t('actions.close') : t('actions.cancel') }}
          </UButton>
          <UiButton
            v-if="!inviteResult"
            icon="i-lucide-user-plus"
            :loading="isInviting"
            :disabled="!inviteForm.userId || !inviteForm.role"
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
import type { SharedSpace, SpaceInvite, SpaceRole } from '@haex-space/vault-sdk'

const { t } = useI18n()
const { add } = useToast()
const { copy } = useClipboard()

const spacesStore = useSpacesStore()
const syncBackendsStore = useSyncBackendsStore()

const { spaces } = storeToRefs(spacesStore)
const { backends: syncBackends } = storeToRefs(syncBackendsStore)

// Loading states
const isLoadingSpaces = ref(false)
const isCreating = ref(false)
const isJoining = ref(false)
const isInviting = ref(false)

// Dialog visibility
const showCreateDialog = ref(false)
const showJoinDialog = ref(false)
const showInviteDialog = ref(false)
const showDeleteConfirm = ref(false)
const showLeaveConfirm = ref(false)

// Create form
const createForm = reactive({
  name: '',
  serverUrl: '',
})

// Join form
const joinInviteJson = ref('')

// Invite form
const inviteForm = reactive({
  userId: '',
  role: 'member' as SpaceRole,
})
const inviteResult = ref('')
const inviteSpaceId = ref('')
const inviteServerUrl = ref('')

// Delete/Leave target
const targetSpace = ref<SharedSpace | null>(null)

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

// Role options for invite
const roleOptions = computed(() => [
  { label: t('roles.admin'), value: 'admin' },
  { label: t('roles.member'), value: 'member' },
  { label: t('roles.viewer'), value: 'viewer' },
])

// Helper: badge color for role
const roleBadgeColor = (role: SpaceRole) => {
  switch (role) {
    case 'admin': return 'error' as const
    case 'member': return 'primary' as const
    case 'viewer': return 'neutral' as const
  }
}

// Format date
const formatDate = (dateStr: string) => {
  return new Date(dateStr).toLocaleDateString()
}

// Load spaces on mount
onMounted(async () => {
  await loadSpacesAsync()
})

const loadSpacesAsync = async () => {
  isLoadingSpaces.value = true
  try {
    // Load from all backends that have a serverUrl
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
  if (!createForm.name || !createForm.serverUrl) return

  isCreating.value = true
  try {
    // TODO: password parameter is accepted by the store but not used internally
    const createdSpace = await spacesStore.createSpaceAsync(createForm.serverUrl, createForm.name, '')

    add({
      title: t('success.created'),
      color: 'success',
    })

    showCreateDialog.value = false
    const serverUrl = createForm.serverUrl
    createForm.name = ''
    createForm.serverUrl = ''

    // Open invite dialog for the newly created space
    openInviteDialog({ ...createdSpace, role: 'admin' as SpaceRole }, serverUrl)
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
  if (!joinInviteJson.value) return

  isJoining.value = true
  try {
    let invite: SpaceInvite
    try {
      invite = JSON.parse(joinInviteJson.value)
    } catch {
      add({ title: t('errors.invalidJson'), color: 'error' })
      return
    }
    if (!invite.spaceId || !invite.serverUrl || !invite.accessToken || !invite.encryptedSpaceKey) {
      add({ title: t('errors.invalidInvite'), color: 'error' })
      return
    }

    const { spaceId } = await spacesStore.joinSpaceFromInviteAsync(invite)

    // Create a sync backend for this space
    await syncBackendsStore.addBackendAsync({
      name: `Space ${spaceId.slice(0, 8)}`,
      serverUrl: invite.serverUrl,
      vaultId: invite.spaceId,
      type: 'space',
      spaceId: invite.spaceId,
      spaceToken: invite.accessToken,
      enabled: true,
    })

    add({
      title: t('success.joined'),
      color: 'success',
    })

    showJoinDialog.value = false
    joinInviteJson.value = ''

    // Reload spaces
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

// Open invite dialog
const openInviteDialog = (space: SharedSpace, knownServerUrl?: string) => {
  inviteSpaceId.value = space.id
  inviteForm.userId = ''
  inviteForm.role = 'member'
  inviteResult.value = ''

  if (knownServerUrl) {
    inviteServerUrl.value = knownServerUrl
  } else {
    const serverUrl = getServerUrlForSpace(space.id)
    if (!serverUrl) {
      add({ title: t('errors.noServerUrl'), color: 'error' })
      return
    }
    inviteServerUrl.value = serverUrl
  }

  showInviteDialog.value = true
}

// Invite member
const onInviteMemberAsync = async () => {
  if (!inviteForm.userId || !inviteForm.role || !inviteSpaceId.value) return

  isInviting.value = true
  try {
    const invite = await spacesStore.inviteMemberAsync(
      inviteServerUrl.value,
      inviteSpaceId.value,
      inviteForm.userId,
      inviteForm.role,
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

// Copy invite JSON
const copyInvite = () => {
  copy(inviteResult.value)
  add({
    title: t('success.copied'),
    color: 'success',
  })
}

// Close invite dialog
const closeInviteDialog = () => {
  showInviteDialog.value = false
  inviteResult.value = ''
}

// Prepare delete/leave
const prepareDeleteSpace = (space: SharedSpace) => {
  targetSpace.value = space
  showDeleteConfirm.value = true
}

const prepareLeaveSpace = (space: SharedSpace) => {
  targetSpace.value = space
  showLeaveConfirm.value = true
}

// Find server URL for a space
const getServerUrlForSpace = (spaceId: string): string | null => {
  const backend = syncBackends.value.find(b => b.spaceId === spaceId)
  return backend?.serverUrl ?? null
}

// Confirm delete
const onConfirmDeleteAsync = async () => {
  if (!targetSpace.value) return

  try {
    const serverUrl = getServerUrlForSpace(targetSpace.value.id)
    if (!serverUrl) {
      add({ title: t('errors.noServerUrl'), color: 'error' })
      return
    }
    await spacesStore.deleteSpaceAsync(serverUrl, targetSpace.value.id)

    // Remove associated sync backend
    const backend = syncBackends.value.find(b => b.spaceId === targetSpace.value!.id)
    if (backend) {
      await syncBackendsStore.deleteBackendAsync(backend.id)
    }

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
    const serverUrl = getServerUrlForSpace(targetSpace.value.id)
    if (!serverUrl) {
      add({ title: t('errors.noServerUrl'), color: 'error' })
      return
    }
    await spacesStore.leaveSpaceAsync(serverUrl, targetSpace.value.id)

    // Remove associated sync backend
    const backend = syncBackends.value.find(b => b.spaceId === targetSpace.value!.id)
    if (backend) {
      await syncBackendsStore.deleteBackendAsync(backend.id)
    }

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
    createdAt: Erstellt am
  roles:
    admin: Admin
    member: Mitglied
    viewer: Betrachter
  create:
    title: Space erstellen
    description: Erstelle einen neuen geteilten Space
    nameLabel: Name
    serverLabel: Server auswählen
  join:
    title: Space beitreten
    description: Tritt einem Space mit einer Einladung bei
    inviteLabel: Einladungs-JSON einfügen
  invite:
    title: Mitglied einladen
    description: Lade einen Benutzer in diesen Space ein
    userIdLabel: Benutzer-ID
    roleLabel: Rolle auswählen
    resultDescription: Teile dieses Einladungs-JSON mit dem Benutzer
    resultLabel: Einladungs-JSON
  delete:
    title: Space löschen
    description: Möchtest du diesen Space wirklich löschen? Alle Daten werden unwiderruflich entfernt.
  leave:
    title: Space verlassen
    description: Möchtest du diesen Space wirklich verlassen? Du kannst nur durch eine erneute Einladung wieder beitreten.
  actions:
    create: Erstellen
    join: Beitreten
    invite: Einladen
    leave: Verlassen
    delete: Löschen
    cancel: Abbrechen
    close: Schließen
    copy: Kopieren
  success:
    created: Space erstellt
    joined: Space beigetreten
    invited: Einladung erstellt
    deleted: Space gelöscht
    left: Space verlassen
    copied: In Zwischenablage kopiert
  errors:
    createFailed: Space konnte nicht erstellt werden
    joinFailed: Beitritt fehlgeschlagen
    inviteFailed: Einladung fehlgeschlagen
    deleteFailed: Löschen fehlgeschlagen
    leaveFailed: Verlassen fehlgeschlagen
    noPassword: Kein Passwort für diesen Server gefunden
    invalidJson: Ungültiges JSON-Format
    invalidInvite: Unvollständige Einladung
    noServerUrl: Server-URL für diesen Space nicht gefunden
en:
  title: Spaces
  description: Create, manage and join shared spaces
  list:
    title: Your Spaces
    description: Shared spaces for collaboration with others
    empty: No spaces found
    createdAt: Created at
  roles:
    admin: Admin
    member: Member
    viewer: Viewer
  create:
    title: Create Space
    description: Create a new shared space
    nameLabel: Name
    serverLabel: Select server
  join:
    title: Join Space
    description: Join a space using an invitation
    inviteLabel: Paste invite JSON
  invite:
    title: Invite Member
    description: Invite a user to this space
    userIdLabel: User ID
    roleLabel: Select role
    resultDescription: Share this invite JSON with the user
    resultLabel: Invite JSON
  delete:
    title: Delete Space
    description: Do you really want to delete this space? All data will be permanently removed.
  leave:
    title: Leave Space
    description: Do you really want to leave this space? You can only rejoin with a new invitation.
  actions:
    create: Create
    join: Join
    invite: Invite
    leave: Leave
    delete: Delete
    cancel: Cancel
    close: Close
    copy: Copy
  success:
    created: Space created
    joined: Joined space
    invited: Invitation created
    deleted: Space deleted
    left: Left space
    copied: Copied to clipboard
  errors:
    createFailed: Failed to create space
    joinFailed: Failed to join space
    inviteFailed: Failed to invite member
    deleteFailed: Failed to delete space
    leaveFailed: Failed to leave space
    noPassword: No password found for this server
    invalidJson: Invalid JSON format
    invalidInvite: Invalid or incomplete invitation
    noServerUrl: Server URL for this space not found
</i18n>
