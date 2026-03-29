<template>
  <div class="space-y-3">
    <!-- Policy selector -->
    <div class="flex items-center justify-between p-3 rounded-lg border border-default">
      <div class="min-w-0">
        <p class="text-sm font-medium">{{ t('policy.label') }}</p>
        <p class="text-xs text-muted">{{ t('policy.description') }}</p>
      </div>
      <USelectMenu
        :model-value="policyOption"
        :items="policyOptions"
        class="w-44 shrink-0"
        @update:model-value="onPolicyChange"
      />
    </div>

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

    <!-- Invite list -->
    <div
      v-else-if="pendingInvites.length"
      class="space-y-3"
    >
      <div
        v-for="invite in pendingInvites"
        :key="invite.id"
        class="flex flex-col gap-2 p-3 rounded-lg bg-gray-50 dark:bg-gray-800/50"
      >
        <div class="flex flex-col @xs:flex-row @xs:items-center @xs:justify-between gap-2">
          <div class="flex-1 min-w-0">
            <div class="flex items-center gap-2 flex-wrap">
              <p class="font-medium text-sm truncate">
                {{ invite.spaceName || t('invite.unknownSpace') }}
              </p>
              <UBadge
                color="info"
                variant="subtle"
                size="sm"
              >
                {{ t('invite.pending') }}
              </UBadge>
              <UBadge
                v-if="invite.capability"
                color="neutral"
                variant="subtle"
                size="sm"
              >
                {{ invite.capability }}
              </UBadge>
            </div>
            <p class="text-xs text-muted mt-1">
              {{ t('invite.from') }}: {{ invite.inviterLabel || invite.inviterDid }}
            </p>
            <p class="text-xs text-muted">
              {{ t('invite.received') }}: {{ formatDate(invite.createdAt) }}
            </p>
          </div>
          <div class="flex gap-2 @xs:shrink-0">
            <UButton
              color="primary"
              variant="soft"
              icon="i-lucide-check"
              :loading="processingId === invite.id && processingAction === 'accept'"
              :disabled="!!processingId"
              @click="onAcceptAsync(invite)"
            >
              {{ t('actions.accept') }}
            </UButton>
            <UButton
              color="neutral"
              variant="outline"
              icon="i-lucide-x"
              :loading="processingId === invite.id && processingAction === 'decline'"
              :disabled="!!processingId"
              @click="onDeclineAsync(invite)"
            >
              {{ t('actions.decline') }}
            </UButton>
            <UButton
              color="error"
              variant="ghost"
              icon="i-lucide-ban"
              :title="t('actions.block')"
              :disabled="!!processingId"
              @click="onBlockAsync(invite)"
            />
          </div>
        </div>
      </div>
    </div>

    <!-- Empty state -->
    <HaexSystemSettingsLayoutEmpty
      v-else
      :message="t('invite.empty')"
      icon="i-lucide-mail-open"
    />
  </div>
</template>

<script setup lang="ts">
import { eq } from 'drizzle-orm'
import { haexPendingInvites, type SelectHaexPendingInvites } from '~/database/schemas'
import { fetchWithDidAuth } from '@/utils/auth/didAuth'
import { useInvitePolicy } from '@/composables/useInvitePolicy'
import { useMlsDelivery } from '@/composables/useMlsDelivery'

const { t } = useI18n()
const { add } = useToast()

const { currentVault } = storeToRefs(useVaultStore())
const identityStore = useIdentityStore()
const spacesStore = useSpacesStore()
const syncBackendsStore = useSyncBackendsStore()
const { backends: syncBackends } = storeToRefs(syncBackendsStore)

const { blockDid, setPolicy, getPolicy } = useInvitePolicy()

const isLoading = ref(false)
const pendingInvites = ref<SelectHaexPendingInvites[]>([])
const processingId = ref<string | null>(null)
const processingAction = ref<'accept' | 'decline' | null>(null)
const currentPolicy = ref<'all' | 'contacts_only' | 'nobody'>('all')

const policyOptions = computed(() => [
  { label: t('policy.all'), value: 'all' },
  { label: t('policy.contactsOnly'), value: 'contacts_only' },
  { label: t('policy.nobody'), value: 'nobody' },
])

const policyOption = computed(() =>
  policyOptions.value.find(o => o.value === currentPolicy.value),
)

const onPolicyChange = async (option: { label: string; value: string }) => {
  const newPolicy = option.value as 'all' | 'contacts_only' | 'nobody'
  try {
    await setPolicy(newPolicy)
    currentPolicy.value = newPolicy
    add({ title: t('policy.updated'), color: 'success' })
  } catch (error) {
    console.error('Failed to update policy:', error)
    add({ title: t('policy.updateFailed'), color: 'error' })
  }
}

const formatDate = (dateStr: string) => {
  return new Date(dateStr).toLocaleDateString()
}

const getDb = () => currentVault.value?.drizzle

const loadInvitesAsync = async () => {
  const db = getDb()
  if (!db) return

  isLoading.value = true
  try {
    const rows = await db
      .select()
      .from(haexPendingInvites)
      .where(eq(haexPendingInvites.status, 'pending'))

    pendingInvites.value = rows
    currentPolicy.value = await getPolicy()
  } finally {
    isLoading.value = false
  }
}

/** Find the server URL for a space from sync backends */
const getServerUrlForSpace = (spaceId: string): string | undefined => {
  const backend = syncBackends.value.find(b => b.spaceId === spaceId)
  return backend?.serverUrl
}

/** Get the first available identity for auth */
const getIdentityAsync = async () => {
  await identityStore.loadIdentitiesAsync()
  const identity = identityStore.identities[0]
  if (!identity) throw new Error('No identity available')
  return identity
}

const onAcceptAsync = async (invite: SelectHaexPendingInvites) => {
  processingId.value = invite.id
  processingAction.value = 'accept'

  try {
    const identity = await getIdentityAsync()
    const serverUrl = getServerUrlForSpace(invite.spaceId)
    if (!serverUrl) {
      add({ title: t('errors.noServer'), color: 'error' })
      return
    }

    // Accept invite on server + upload MLS KeyPackages
    const delivery = useMlsDelivery(serverUrl, invite.spaceId, {
      privateKey: identity.privateKey,
      did: identity.did,
    })
    await delivery.acceptInviteAsync(invite.id)

    // Mark invite as accepted in local DB
    const db = getDb()
    if (db) {
      await db.update(haexPendingInvites).set({
        status: 'accepted',
        respondedAt: new Date().toISOString(),
      }).where(eq(haexPendingInvites.id, invite.id))
    }

    // Reload spaces to include the newly joined space
    await spacesStore.loadSpacesFromDbAsync()

    add({ title: t('success.accepted'), color: 'success' })
    await loadInvitesAsync()
  } catch (error) {
    console.error('Failed to accept invite:', error)
    add({
      title: t('errors.acceptFailed'),
      description: error instanceof Error ? error.message : undefined,
      color: 'error',
    })
  } finally {
    processingId.value = null
    processingAction.value = null
  }
}

const onDeclineAsync = async (invite: SelectHaexPendingInvites) => {
  processingId.value = invite.id
  processingAction.value = 'decline'

  try {
    const identity = await getIdentityAsync()
    const serverUrl = getServerUrlForSpace(invite.spaceId)
    if (!serverUrl) {
      // No server URL — just mark as declined locally
      const db = getDb()
      if (db) {
        await db.update(haexPendingInvites).set({
          status: 'declined',
          respondedAt: new Date().toISOString(),
        }).where(eq(haexPendingInvites.id, invite.id))
      }
      add({ title: t('success.declined'), color: 'success' })
      await loadInvitesAsync()
      return
    }

    const response = await fetchWithDidAuth(
      `${serverUrl}/spaces/${invite.spaceId}/invites/${invite.id}/decline`,
      identity.privateKey,
      identity.did,
      'decline-invite',
      { method: 'POST', headers: { 'Content-Type': 'application/json' } },
    )

    if (!response.ok) {
      const error = await response.json().catch(() => ({}))
      throw new Error(error.error || response.statusText)
    }

    // Mark invite as declined
    const db = getDb()
    if (db) {
      await db.update(haexPendingInvites).set({
        status: 'declined',
        respondedAt: new Date().toISOString(),
      }).where(eq(haexPendingInvites.id, invite.id))
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
  } finally {
    processingId.value = null
    processingAction.value = null
  }
}

const onBlockAsync = async (invite: SelectHaexPendingInvites) => {
  try {
    await blockDid(invite.inviterDid, invite.inviterLabel ?? undefined)

    // Also decline the invite
    const db = getDb()
    if (db) {
      await db.update(haexPendingInvites).set({
        status: 'declined',
        respondedAt: new Date().toISOString(),
      }).where(eq(haexPendingInvites.id, invite.id))
    }

    add({ title: t('success.blocked'), color: 'success' })
    await loadInvitesAsync()
  } catch (error) {
    console.error('Failed to block DID:', error)
    add({
      title: t('errors.blockFailed'),
      description: error instanceof Error ? error.message : undefined,
      color: 'error',
    })
  }
}

onMounted(() => loadInvitesAsync())

defineExpose({ loadInvitesAsync })
</script>

<i18n lang="yaml">
de:
  policy:
    label: Einladungsrichtlinie
    description: Bestimme, wer dir Einladungen senden darf
    all: Alle
    contactsOnly: Nur Kontakte
    nobody: Niemand
    updated: Richtlinie aktualisiert
    updateFailed: Richtlinie konnte nicht aktualisiert werden
  invite:
    pending: Ausstehend
    from: Von
    received: Empfangen
    empty: Keine ausstehenden Einladungen
    unknownSpace: Unbekannter Space
  actions:
    accept: Annehmen
    decline: Ablehnen
    block: Blockieren
  success:
    accepted: Einladung angenommen
    declined: Einladung abgelehnt
    blocked: Absender blockiert
  errors:
    acceptFailed: Einladung konnte nicht angenommen werden
    declineFailed: Einladung konnte nicht abgelehnt werden
    blockFailed: Blockieren fehlgeschlagen
    noServer: Kein Server für diesen Space gefunden
en:
  policy:
    label: Invite policy
    description: Control who can send you invitations
    all: Everyone
    contactsOnly: Contacts only
    nobody: Nobody
    updated: Policy updated
    updateFailed: Failed to update policy
  invite:
    pending: Pending
    from: From
    received: Received
    empty: No pending invitations
    unknownSpace: Unknown space
  actions:
    accept: Accept
    decline: Decline
    block: Block
  success:
    accepted: Invitation accepted
    declined: Invitation declined
    blocked: Sender blocked
  errors:
    acceptFailed: Failed to accept invitation
    declineFailed: Failed to decline invitation
    blockFailed: Failed to block sender
    noServer: No server found for this space
</i18n>
