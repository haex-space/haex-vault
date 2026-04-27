<template>
  <Transition
    :name="direction === 'back' ? 'slide-back' : 'slide-forward'"
    mode="out-in"
  >
    <div
      :key="activeView"
      class="h-full"
    >
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
            data-testid="spaces-create-trigger"
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
            @accept="
              onAcceptInviteAsync(
                entry.kind === 'pending' ? entry.invite : undefined,
              )
            "
            @decline="
              onDeclineInviteAsync(
                entry.kind === 'pending' ? entry.invite : undefined,
              )
            "
            @edit="openEditDialog"
            @add-share="onAddShareAsync"
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

        <SpaceCreateDialog
          v-model:open="showCreateDialog"
          :origin-url-options="originUrlOptions"
          :owner-identity-options="ownerIdentityOptions"
          :default-owner-identity-id="defaultOwnerIdentityId"
          :submitting="isCreating"
          @submit="onCreateSpaceAsync"
          @navigate-to-sync="onNavigateToSync"
        />

        <SpaceJoinDialog
          v-model:open="showJoinDialog"
          :initial-invite-link="props.inviteLink"
          :submitting="isJoining"
          @submit="onJoinSpaceAsync"
        />
      </HaexSystemSettingsLayout>

      <!-- Dialogs (rendered outside layout so they work in both index and detail views) -->
      <SpaceEditDialog
        v-model:open="showEditDialog"
        :space="editingSpace"
        :server-options="editServerOptions"
        :space-is-local="editingSpaceIsLocal"
        :submitting="isSavingEdit"
        @submit="onSaveEditAsync"
        @navigate-to-sync="onNavigateToSync"
      />

      <SpaceInviteDialog
        v-model:open="showInviteDialog"
        :space-id="inviteSpaceId"
        :origin-url="inviteServerUrl"
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
  </Transition>
</template>

<script setup lang="ts">
import { didKeyToPublicKeyAsync } from '@haex-space/vault-sdk'
import { invoke } from '@tauri-apps/api/core'
import { SettingsCategory } from '~/config/settingsCategories'
import type { SpaceWithType } from '@/stores/spaces'
import type { SelectHaexPendingInvites } from '~/database/schemas'
import SpaceListItem from './spaces/SpaceListItem.vue'
import SpaceDetail from './spaces/SpaceDetail.vue'
import SpaceInviteDialog from './spaces/SpaceInviteDialog.vue'
import SpaceCreateDialog, {
  type CreateSpacePayload,
} from './spaces/SpaceCreateDialog.vue'
import SpaceJoinDialog from './spaces/SpaceJoinDialog.vue'
import SpaceEditDialog, {
  type EditSpacePayload,
} from './spaces/SpaceEditDialog.vue'
import { parseInviteTokenLink, parseLocalInviteLink } from '~/utils/inviteLink'
import { SpaceType, SpaceStatus } from '~/database/constants'
import {
  useSpaceInvites,
  type InvitePolicyValue,
} from '@/composables/useSpaceInvites'
import { useCurrentIdentity } from '@/composables/useCurrentIdentity'
import { useOperationErrorToast } from '@/composables/useOperationErrorToast'
import { useSpaceShares } from '@/composables/useSpaceShares'

type SpaceListEntry =
  | { kind: 'active'; space: SpaceWithType }
  | { kind: 'pending'; space: SpaceWithType; invite: SelectHaexPendingInvites }

const props = defineProps<{
  inviteLink?: string
}>()

const { t } = useI18n()
const { add } = useToast()

const tabId = inject<string>('haex-tab-id')!
const { activeView, navigationContext, direction, navigateTo, goBack } =
  useDrillDownNavigation<'index' | 'detail'>('index', 'spaces', tabId)
const selectedSpaceId = computed(
  () => navigationContext.value.spaceId as string | null,
)

const openSpaceDetail = (space: SpaceWithType) => {
  navigateTo('detail', { spaceId: space.id })
}

const spacesStore = useSpacesStore()
const syncBackendsStore = useSyncBackendsStore()
const identityStore = useIdentityStore()
const windowManager = useWindowManagerStore()

const { ensureCurrentIdentityAsync, ensureCurrentIdentityIdAsync } =
  useCurrentIdentity()
const { showOperationError } = useOperationErrorToast()
const { addShareAsync: addShareToSpaceAsync } = useSpaceShares()

const onAddShareAsync = async (payload: {
  space: SpaceWithType
  type: 'folder' | 'file'
}) => {
  await addShareToSpaceAsync({ spaceId: payload.space.id, type: payload.type })
}

const { activeSpaces, spaces } = storeToRefs(spacesStore)
const { backends: syncBackends } = storeToRefs(syncBackendsStore)

// =========================================================================
// Pending invites (state + accept/decline/policy in composable)
// =========================================================================

const {
  pendingInvites,
  currentPolicy,
  loadInvitesAsync,
  changePolicyAsync,
  acceptInviteAsync,
  declineInviteAsync,
  listenForPushInvitesAsync,
} = useSpaceInvites()

const policyOptions = computed(() => [
  { label: t('policy.all'), value: 'all' as InvitePolicyValue },
  { label: t('policy.contactsOnly'), value: 'contacts_only' as InvitePolicyValue },
  { label: t('policy.nobody'), value: 'nobody' as InvitePolicyValue },
])

const policyOption = computed(() =>
  policyOptions.value.find((o) => o.value === currentPolicy.value),
)

const onPolicyChangeAsync = async (option: {
  label: string
  value: InvitePolicyValue
}) => {
  try {
    await changePolicyAsync(option.value)
  } catch (error) {
    console.error('Failed to update policy:', error)
    showOperationError(error, 'errors.policyFailed')
  }
}

const onAcceptInviteAsync = async (invite?: SelectHaexPendingInvites) => {
  if (!invite) return
  try {
    await acceptInviteAsync(invite)
    add({ title: t('success.accepted'), color: 'success' })
  } catch (error) {
    console.error('Failed to accept invite:', error)
    showOperationError(error, 'errors.acceptFailed')
  }
}

const onDeclineInviteAsync = async (invite?: SelectHaexPendingInvites) => {
  if (!invite) return
  try {
    await declineInviteAsync(invite)
    add({ title: t('success.declined'), color: 'success' })
  } catch (error) {
    console.error('Failed to decline invite:', error)
    showOperationError(error, 'errors.declineFailed')
  }
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
      ownerIdentityId: '',
      originUrl: invite.originUrl || '',
      createdAt: invite.createdAt || '',
      capabilities: [],
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
// Loading states & dialog visibility
// =========================================================================

const isLoadingSpaces = ref(false)
const isCreating = ref(false)
const isJoining = ref(false)
const isSavingEdit = ref(false)

const showCreateDialog = ref(false)
const showJoinDialog = ref(false)
const showInviteDialog = ref(false)
const showEditDialog = ref(false)
const showDeleteConfirm = ref(false)
const showLeaveConfirm = ref(false)

// Invite dialog state
const inviteSpaceId = ref('')
const inviteServerUrl = ref('')
const inviteMode = ref<'contact' | 'link'>('contact')
const inviteIdentityId = ref('')

// Edit dialog state
const editingSpace = ref<SpaceWithType | null>(null)
const editingSpaceIsLocal = computed(() => {
  const space = spaces.value.find((s) => s.id === editingSpace.value?.id)
  return space?.type === SpaceType.LOCAL
})

const editServerOptions = computed(() => {
  const options = [{ label: t('edit.noServer'), value: '' }]
  for (const backend of syncBackends.value) {
    options.push({ label: backend.name, value: backend.homeServerUrl })
  }
  return options
})

// Delete/Leave target
const targetSpace = ref<SpaceWithType | null>(null)

// Server URL options (for create dialog)
const originUrlOptions = computed(() => {
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

const ownerIdentityOptions = computed(() =>
  identityStore.ownIdentities.map((identity) => ({
    label: `${identity.name} (${identity.did.slice(0, 24)}...)`,
    value: identity.id,
  })),
)

const defaultOwnerIdentityId = computed(() =>
  spaces.value.find((s) => s.type === SpaceType.VAULT)?.ownerIdentityId
    || identityStore.ownIdentities[0]?.id
    || '',
)

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
    showJoinDialog.value = true
  }

  unlistenPushInvite = await listenForPushInvitesAsync()
})

onUnmounted(() => {
  unlistenPushInvite?.()
})

const loadSpacesAsync = async () => {
  isLoadingSpaces.value = true
  try {
    await identityStore.loadIdentitiesAsync()
    await spacesStore.ensureDefaultSpaceAsync()

    // Unconditional reload: ensureDefaultSpaceAsync only refreshes the store
    // when its own probe (the first local space) is missing, so a row that
    // was inserted while the settings window was closed (e.g. a QUIC invite
    // accepted from a different view, then user re-opens settings) would
    // never reach activeSpaces. Always reload here so the on-mount contract
    // is "the store reflects the current DB state."
    await spacesStore.loadSpacesFromDbAsync()

    for (const backend of syncBackends.value) {
      if (backend.homeServerUrl) {
        await spacesStore.listSpacesAsync(
          backend.homeServerUrl,
          backend.identityId,
        )
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
// Create / Join / Edit / Invite / Delete / Leave
// =========================================================================

const onCreateSpaceAsync = async (payload: CreateSpacePayload) => {
  isCreating.value = true
  try {
    if (payload.type === SpaceType.LOCAL) {
      await spacesStore.createLocalSpaceAsync(payload.name, payload.ownerIdentityId)
      add({ title: t('success.created'), color: 'success' })
      showCreateDialog.value = false
    } else {
      const originUrl = payload.originUrl?.value
      if (!originUrl) {
        add({ title: t('errors.noServer'), color: 'error' })
        return
      }

      const identityId = await ensureCurrentIdentityIdAsync()
      const createdSpace = await spacesStore.createSpaceAsync(
        originUrl,
        payload.name,
        t('create.defaultSelfLabel'),
        identityId,
      )
      add({ title: t('success.created'), color: 'success' })
      showCreateDialog.value = false

      openInviteDialog({
        ...createdSpace,
        name: payload.name,
        originUrl: originUrl,
        createdAt: new Date().toISOString(),
        capabilities: [],
      } as unknown as SpaceWithType)
    }
  } catch (error) {
    console.error('Failed to create space:', error)
    showOperationError(error, 'errors.createFailed')
  } finally {
    isCreating.value = false
  }
}

const onJoinSpaceAsync = async (payload: { inviteLink: string }) => {
  isJoining.value = true
  try {
    const localLink = parseLocalInviteLink(payload.inviteLink)
    if (localLink) {
      const identity = await ensureCurrentIdentityAsync()

      let lastError: Error | null = null
      for (const endpointId of localLink.spaceEndpoints) {
        try {
          await invoke('local_delivery_claim_invite', {
            leaderEndpointId: endpointId,
            leaderRelayUrl: null,
            spaceId: localLink.spaceId,
            tokenId: localLink.tokenId,
            identityDid: identity.did,
            label: identity.name || null,
            identityPublicKey: await didKeyToPublicKeyAsync(identity.did),
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
      await spacesStore.loadSpacesFromDbAsync()
      return
    }

    const tokenLink = parseInviteTokenLink(payload.inviteLink)
    if (!tokenLink) {
      add({ title: t('errors.invalidInviteLink'), color: 'error' })
      return
    }

    const identityId = await ensureCurrentIdentityIdAsync()

    await spacesStore.claimInviteTokenAsync(
      tokenLink.originUrl,
      tokenLink.spaceId,
      tokenLink.tokenId,
      identityId,
    )

    await syncBackendsStore.addBackendAsync({
      name: `Space ${tokenLink.spaceId.slice(0, 8)}`,
      homeServerUrl: tokenLink.originUrl,
      spaceId: tokenLink.spaceId,
      identityId,
      enabled: true,
    })

    add({ title: t('success.joined'), color: 'success' })
    showJoinDialog.value = false
    await loadSpacesAsync()
  } catch (error) {
    console.error('Failed to join space:', error)
    showOperationError(error, 'errors.joinFailed')
  } finally {
    isJoining.value = false
  }
}

const openEditDialog = (space: SpaceWithType) => {
  editingSpace.value = space
  showEditDialog.value = true
}

const onSaveEditAsync = async (payload: EditSpacePayload) => {
  if (!editingSpace.value) return

  isSavingEdit.value = true
  try {
    const space = editingSpace.value
    const oldServerUrl = space.originUrl

    if (payload.name !== space.name) {
      await spacesStore.updateSpaceNameAsync(space.id, payload.name)
    }

    if (payload.originUrl !== oldServerUrl) {
      // Identity only required when attaching to a server — clearing the
      // server (going back to local) needs no identity lookup.
      const identityId = payload.originUrl
        ? await ensureCurrentIdentityIdAsync()
        : (identityStore.ownIdentities[0]?.id ?? '')
      await spacesStore.migrateSpaceServerAsync(
        space.id,
        oldServerUrl,
        payload.originUrl,
        identityId,
      )
    }

    add({ title: t('success.updated'), color: 'success' })
    showEditDialog.value = false
  } catch (error) {
    console.error('Failed to update space:', error)
    showOperationError(error, 'errors.updateFailed')
  } finally {
    isSavingEdit.value = false
  }
}

const getIdentityForSpace = (spaceServerUrl: string): string | undefined => {
  const backend = syncBackends.value.find(
    (b) => b.homeServerUrl === spaceServerUrl,
  )
  return backend?.identityId ?? undefined
}

const openInviteDialog = (
  space: SpaceWithType,
  mode: 'contact' | 'link' = 'contact',
) => {
  inviteSpaceId.value = space.id
  inviteServerUrl.value = space.originUrl
  inviteIdentityId.value = getIdentityForSpace(space.originUrl) ?? ''
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
    await spacesStore.deleteSpaceAsync(
      targetSpace.value.originUrl,
      targetSpace.value.id,
    )
    add({ title: t('success.deleted'), color: 'success' })
    showDeleteConfirm.value = false
    targetSpace.value = null
  } catch (error) {
    console.error('Failed to delete space:', error)
    showOperationError(error, 'errors.deleteFailed')
  }
}

const onConfirmLeaveAsync = async () => {
  if (!targetSpace.value) return
  try {
    const identityId = getIdentityForSpace(targetSpace.value.originUrl)
    if (!identityId) {
      add({ title: t('errors.noIdentity'), color: 'error' })
      return
    }
    await spacesStore.leaveSpaceAsync(
      targetSpace.value.originUrl,
      targetSpace.value.id,
      identityId,
    )
    add({ title: t('success.left'), color: 'success' })
    showLeaveConfirm.value = false
    targetSpace.value = null
  } catch (error) {
    console.error('Failed to leave space:', error)
    showOperationError(error, 'errors.leaveFailed')
  }
}
</script>

<i18n lang="yaml">
de:
  title: Spaces
  description: Erstelle, verwalte und tritt geteilten Spaces bei
  policy:
    label: 'Einladungen erlaubt von:'
    all: Alle
    contactsOnly: Nur Kontakte
    nobody: Niemand
  list:
    empty: Keine Spaces vorhanden
  create:
    localOnly: Lokal (ohne Server)
    defaultSelfLabel: Ich
  edit:
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
  success:
    created: Space erstellt
    joined: Space beigetreten
    deleted: Space gelöscht
    updated: Space aktualisiert
    left: Space verlassen
    accepted: Einladung angenommen
    declined: Einladung abgelehnt
  errors:
    updateFailed: Space konnte nicht aktualisiert werden
    createFailed: Space konnte nicht erstellt werden
    joinFailed: Beitritt fehlgeschlagen
    deleteFailed: Löschen fehlgeschlagen
    leaveFailed: Verlassen fehlgeschlagen
    invalidInviteLink: Ungültiger Einladungslink
    noIdentity: Keine Identität verfügbar
    noServer: Kein Server ausgewählt
    acceptFailed: Einladung konnte nicht angenommen werden
    declineFailed: Einladung konnte nicht abgelehnt werden
    policyFailed: Richtlinie konnte nicht aktualisiert werden
en:
  title: Spaces
  description: Create, manage and join shared spaces
  policy:
    label: 'Invitations allowed from:'
    all: Everyone
    contactsOnly: Contacts only
    nobody: Nobody
  list:
    empty: No spaces found
  create:
    localOnly: Local (no server)
    defaultSelfLabel: Me
  edit:
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
  success:
    created: Space created
    joined: Joined space
    deleted: Space deleted
    updated: Space updated
    left: Left space
    accepted: Invitation accepted
    declined: Invitation declined
  errors:
    createFailed: Failed to create space
    updateFailed: Failed to update space
    joinFailed: Failed to join space
    deleteFailed: Failed to delete space
    leaveFailed: Failed to leave space
    invalidInviteLink: Invalid invite link
    noIdentity: No identity available
    noServer: No server selected
    acceptFailed: Failed to accept invitation
    declineFailed: Failed to decline invitation
    policyFailed: Failed to update policy
</i18n>
