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
      <SpacesIndexView
        v-else
        v-model:show-create-dialog="showCreateDialog"
        v-model:show-join-dialog="showJoinDialog"
        :is-loading-spaces="isLoadingSpaces"
        :space-list-entries="spaceListEntries"
        :first-active-space-id="firstActiveSpaceId"
        :policy-option="policyOption"
        :policy-options="policyOptions"
        :origin-url-options="originUrlOptions"
        :owner-identity-options="ownerIdentityOptions"
        :default-owner-identity-id="defaultOwnerIdentityId"
        :is-creating="isCreating"
        :is-joining="isJoining"
        :invite-link="props.inviteLink"
        @policy-change="onPolicyChangeAsync"
        @create-space="onCreateSpaceAsync"
        @join-space="onJoinSpaceAsync"
        @navigate-to-sync="onNavigateToSync"
        @select-space="openSpaceDetail"
        @accept-invite="onAcceptInviteAsync"
        @decline-invite="onDeclineInviteAsync"
        @edit-space="openEditDialog"
        @add-share="onAddShareAsync"
        @invite-contact="openInviteDialog($event, 'contact')"
        @invite-link="openInviteDialog($event, 'link')"
        @delete-space="prepareDeleteSpace"
        @leave-space="prepareLeaveSpace"
      />

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
import SpaceDetail from './spaces/SpaceDetail.vue'
import SpaceInviteDialog from './spaces/SpaceInviteDialog.vue'
import SpaceEditDialog from './spaces/SpaceEditDialog.vue'
import SpacesIndexView, {
  type SpaceListEntry,
} from './spaces/SpacesIndexView.vue'
import { SpaceType, SpaceStatus } from '~/database/constants'
import { useSpacesActions } from '@/composables/useSpacesActions'
import type { SpaceWithType } from '@/stores/spaces/types'

const props = defineProps<{
  inviteLink?: string
}>()

const { t } = useI18n()

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
const { activeSpaces } = storeToRefs(spacesStore)

const {
  // Pending invites
  pendingInvites,
  listenForPushInvitesAsync,
  onPolicyChangeAsync,
  onAcceptInviteAsync,
  onDeclineInviteAsync,
  policyOption,
  policyOptions,
  // Loading
  isLoadingSpaces,
  isCreating,
  isJoining,
  isSavingEdit,
  // Dialog visibility
  showCreateDialog,
  showJoinDialog,
  showInviteDialog,
  showEditDialog,
  showDeleteConfirm,
  showLeaveConfirm,
  // Invite dialog state
  inviteSpaceId,
  inviteServerUrl,
  inviteMode,
  inviteIdentityId,
  // Edit dialog state
  editingSpace,
  editingSpaceIsLocal,
  editServerOptions,
  // Computed options
  originUrlOptions,
  ownerIdentityOptions,
  defaultOwnerIdentityId,
  // Actions
  loadSpacesAsync,
  onCreateSpaceAsync,
  onJoinSpaceAsync,
  onSaveEditAsync,
  onAddShareAsync,
  openEditDialog,
  openInviteDialog,
  prepareDeleteSpace,
  prepareLeaveSpace,
  onConfirmDeleteAsync,
  onConfirmLeaveAsync,
  onNavigateToSync,
} = useSpacesActions()

// =========================================================================
// Unified space list
// =========================================================================

const spaceListEntries = computed((): SpaceListEntry[] => {
  const entries: SpaceListEntry[] = []
  // A space is either pending OR active for this user, never both. After
  // accept, `pendingInvites` is refreshed at the end of acceptInviteAsync,
  // but `activeSpaces` updates earlier in the same chain — and any racy
  // path (re-fired `push-invite-received` event, mounted-but-not-yet-loaded
  // component) can leave a stale pending entry around. Without this guard,
  // the v-for emits two SpaceListItems sharing the same `space.id`, the
  // data-testid collides (`space-card-<id>` appears twice), and tests that
  // use `document.querySelector` get the pending one first — see
  // haex-e2e-tests/tests/spaces/invitations/quic-invite-flow.spec.ts:1498.
  const activeSpaceIds = new Set(activeSpaces.value.map((s) => s.id))

  // Pending invites first — construct space from invite metadata
  // (no dummy entry in haex_spaces to avoid CRDT tombstone issues)
  for (const invite of pendingInvites.value) {
    if (activeSpaceIds.has(invite.spaceId)) continue
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

// Anchor the tour at the first ACTIVE card, regardless of where pending invites
// sit in spaceListEntries. Using a raw idx===0 would silently miss the anchor
// whenever a pending invite occupies the top of the list — the onboarding-tour
// stops `space-invite` / `space-add-share` would never resolve.
const firstActiveSpaceId = computed(
  () => activeSpaces.value[0]?.id ?? null,
)

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
    noIdentityForOrigin: 'Für Server {origin} ist keine Identität konfiguriert. Prüfe deine Sync-Backends.'
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
    noIdentityForOrigin: 'No identity is configured for server {origin}. Check your sync backends.'
    noServer: No server selected
    acceptFailed: Failed to accept invitation
    declineFailed: Failed to decline invitation
    policyFailed: Failed to update policy
</i18n>
