import { didKeyToPublicKeyAsync } from '@haex-space/vault-sdk'
import { invoke } from '@tauri-apps/api/core'
import { SettingsCategory } from '~/config/settingsCategories'
import { SpaceType } from '~/database/constants'
import { parseInviteTokenLink, parseLocalInviteLink } from '~/utils/inviteLink'
import { useCurrentIdentity } from '@/composables/useCurrentIdentity'
import { useOperationErrorToast } from '@/composables/useOperationErrorToast'
import { useSpaceShares } from '@/composables/useSpaceShares'
import {
  useSpaceInvites,
  type InvitePolicyValue,
} from '@/composables/useSpaceInvites'
import type { SpaceWithType } from '@/stores/spaces'
import type { SelectHaexPendingInvites } from '~/database/schemas'
import type { CreateSpacePayload } from '@/components/haex/system/settings/spaces/SpaceCreateDialog.vue'
import type { EditSpacePayload } from '@/components/haex/system/settings/spaces/SpaceEditDialog.vue'

export const useSpacesActions = () => {
  const { t } = useI18n()
  const { add } = useToast()

  const spacesStore = useSpacesStore()
  const syncBackendsStore = useSyncBackendsStore()
  const identityStore = useIdentityStore()
  const windowManager = useWindowManagerStore()

  const { ensureCurrentIdentityAsync, ensureCurrentIdentityIdAsync } =
    useCurrentIdentity()
  const { showOperationError } = useOperationErrorToast()
  const { addShareAsync: addShareToSpaceAsync } = useSpaceShares()

  const { spaces } = storeToRefs(spacesStore)
  const { backends: syncBackends } = storeToRefs(syncBackendsStore)

  // =========================================================================
  // Pending invites (state + accept/decline/policy from composable)
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
  // Load
  // =========================================================================

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

  const onCreateSpaceAsync = async (payload: CreateSpacePayload) => {
    isCreating.value = true
    try {
      if (payload.type === SpaceType.LOCAL) {
        const { id } = await spacesStore.createLocalSpaceAsync(payload.name, payload.ownerIdentityId)
        add({ title: t('success.created'), color: 'success' })
        showCreateDialog.value = false
        // Open the Space-Publishing dialog so the user can pick which of
        // their devices should be reachable in the freshly created space.
        useSpacePublishingStore().openForNewSpace(id)
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

  const onAddShareAsync = async (payload: {
    space: SpaceWithType
    type: 'folder' | 'file'
  }) => {
    await addShareToSpaceAsync({ spaceId: payload.space.id, type: payload.type })
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
      const isLocalOnly = targetSpace.value.type === SpaceType.LOCAL
      let identityId: string | null = null
      if (!isLocalOnly) {
        // Non-local spaces must carry their origin URL — without it we can't
        // route the DELETE to the home server and would silently degrade to a
        // local-only delete, leaving orphan rows on the leader.
        if (!targetSpace.value.originUrl) {
          console.error('Leave space aborted: non-local space missing originUrl', {
            spaceId: targetSpace.value.id,
            spaceType: targetSpace.value.type,
          })
          add({
            title: t('errors.deleteFailed'),
            description: t('errors.spaceMissingOrigin', {
              id: targetSpace.value.id,
            }),
            color: 'error',
          })
          return
        }
        identityId = getIdentityForSpace(targetSpace.value.originUrl) ?? null
        if (!identityId) {
          console.error('Leave space aborted: no identity for origin', {
            spaceId: targetSpace.value.id,
            originUrl: targetSpace.value.originUrl,
            availableBackends: syncBackends.value.map((b) => ({
              homeServerUrl: b.homeServerUrl,
              identityId: b.identityId,
            })),
          })
          add({
            title: t('errors.noIdentity'),
            description: t('errors.noIdentityForOrigin', {
              origin: targetSpace.value.originUrl,
            }),
            color: 'error',
          })
          return
        }
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

  return {
    // Pending invites surface
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
    getIdentityForSpace,
  }
}
