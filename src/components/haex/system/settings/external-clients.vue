<template>
  <div>
    <div class="p-6 border-b border-base-content/10">
      <h2 class="text-2xl font-bold">
        {{ t('title') }}
      </h2>
      <p class="text-sm text-gray-500 dark:text-gray-400 mt-1">
        {{ t('description') }}
      </p>
    </div>

    <div class="p-6 space-y-6">
      <div v-if="loading" class="flex justify-center py-8">
        <UIcon
          name="i-heroicons-arrow-path"
          class="w-8 h-8 animate-spin text-primary"
        />
      </div>

      <template v-else>
        <!-- Authorized Clients Section (Permanent) -->
        <div>
          <h3 class="text-lg font-semibold mb-3 flex items-center gap-2">
            <UIcon name="i-heroicons-check-circle" class="w-5 h-5 text-success" />
            {{ t('authorizedClients') }}
          </h3>

          <div
            v-if="!authorizedClients.length"
            class="text-center py-6 text-gray-500 dark:text-gray-400 bg-base-200 rounded-lg"
          >
            {{ t('noAuthorizedClients') }}
          </div>

          <div v-else class="space-y-2">
            <div
              v-for="client in authorizedClients"
              :key="client.id"
              class="p-4 rounded-lg border border-base-300 bg-base-100"
            >
              <div class="flex items-start justify-between gap-4">
                <div class="flex-1 min-w-0">
                  <div class="flex items-center gap-2">
                    <span class="font-semibold">{{ client.clientName }}</span>
                    <UBadge color="success" variant="subtle" size="xs">
                      {{ t('authorized') }}
                    </UBadge>
                  </div>
                  <div class="text-sm text-gray-500 dark:text-gray-400 mt-1">
                    <span>{{ t('extension') }}: {{ getExtensionName(client.extensionId) }}</span>
                  </div>
                  <div class="text-xs text-gray-400 dark:text-gray-500 mt-1 font-mono truncate">
                    {{ client.clientId }}
                  </div>
                  <div v-if="client.authorizedAt" class="text-xs text-gray-400 dark:text-gray-500 mt-1">
                    {{ t('authorizedAt') }}: {{ formatDate(client.authorizedAt) }}
                  </div>
                </div>
                <UButton
                  color="error"
                  variant="ghost"
                  size="sm"
                  :loading="revokingClientId === client.clientId"
                  @click="handleRevokeClient(client)"
                >
                  <UIcon name="i-heroicons-x-mark" class="w-4 h-4" />
                  {{ t('revoke') }}
                </UButton>
              </div>
            </div>
          </div>
        </div>

        <!-- Session Authorizations Section (Temporary) -->
        <div>
          <h3 class="text-lg font-semibold mb-3 flex items-center gap-2">
            <UIcon name="i-heroicons-clock" class="w-5 h-5 text-warning" />
            {{ t('sessionAuthorizations') }}
          </h3>

          <div
            v-if="!sessionAuthorizations.length"
            class="text-center py-6 text-gray-500 dark:text-gray-400 bg-base-200 rounded-lg"
          >
            {{ t('noSessionAuthorizations') }}
          </div>

          <div v-else class="space-y-2">
            <div
              v-for="auth in sessionAuthorizations"
              :key="auth.clientId"
              class="p-4 rounded-lg border border-base-300 bg-base-100"
            >
              <div class="flex items-start justify-between gap-4">
                <div class="flex-1 min-w-0">
                  <div class="flex items-center gap-2">
                    <span class="font-semibold">{{ t('sessionClient') }}</span>
                    <UBadge color="warning" variant="subtle" size="xs">
                      {{ t('sessionOnly') }}
                    </UBadge>
                  </div>
                  <div class="text-sm text-gray-500 dark:text-gray-400 mt-1">
                    <span>{{ t('extension') }}: {{ getExtensionName(auth.extensionId) }}</span>
                  </div>
                  <div class="text-xs text-gray-400 dark:text-gray-500 mt-1 font-mono truncate">
                    {{ auth.clientId }}
                  </div>
                  <div class="text-xs text-gray-400 dark:text-gray-500 mt-1">
                    {{ t('sessionHint') }}
                  </div>
                </div>
                <UButton
                  color="error"
                  variant="ghost"
                  size="sm"
                  :loading="revokingSessionClientId === auth.clientId"
                  @click="handleRevokeSessionAuth(auth)"
                >
                  <UIcon name="i-heroicons-x-mark" class="w-4 h-4" />
                  {{ t('revoke') }}
                </UButton>
              </div>
            </div>
          </div>
        </div>

        <!-- Blocked Clients Section -->
        <div>
          <h3 class="text-lg font-semibold mb-3 flex items-center gap-2">
            <UIcon name="i-heroicons-no-symbol" class="w-5 h-5 text-error" />
            {{ t('blockedClients') }}
          </h3>

          <div
            v-if="!blockedClients.length"
            class="text-center py-6 text-gray-500 dark:text-gray-400 bg-base-200 rounded-lg"
          >
            {{ t('noBlockedClients') }}
          </div>

          <div v-else class="space-y-2">
            <div
              v-for="client in blockedClients"
              :key="client.id"
              class="p-4 rounded-lg border border-base-300 bg-base-100"
            >
              <div class="flex items-start justify-between gap-4">
                <div class="flex-1 min-w-0">
                  <div class="flex items-center gap-2">
                    <span class="font-semibold">{{ client.clientName }}</span>
                    <UBadge color="error" variant="subtle" size="xs">
                      {{ t('blocked') }}
                    </UBadge>
                  </div>
                  <div class="text-xs text-gray-400 dark:text-gray-500 mt-1 font-mono truncate">
                    {{ client.clientId }}
                  </div>
                  <div v-if="client.blockedAt" class="text-xs text-gray-400 dark:text-gray-500 mt-1">
                    {{ t('blockedAt') }}: {{ formatDate(client.blockedAt) }}
                  </div>
                </div>
                <UButton
                  color="success"
                  variant="ghost"
                  size="sm"
                  :loading="unblockingClientId === client.clientId"
                  @click="handleUnblockClient(client)"
                >
                  <UIcon name="i-heroicons-check" class="w-4 h-4" />
                  {{ t('unblock') }}
                </UButton>
              </div>
            </div>
          </div>
        </div>
      </template>
    </div>
  </div>
</template>

<script setup lang="ts">
import type { AuthorizedClient, BlockedClient, SessionAuthorization } from '@haex-space/vault-sdk'

const { t } = useI18n()
const { add } = useToast()
const { getAuthorizedClients, getBlockedClients, revokeClient, unblockClient, getSessionAuthorizations, revokeSessionAuthorization } = useExternalAuth()
const extensionsStore = useExtensionsStore()

const loading = ref(true)
const authorizedClients = ref<AuthorizedClient[]>([])
const blockedClients = ref<BlockedClient[]>([])
const sessionAuthorizations = ref<SessionAuthorization[]>([])
const revokingClientId = ref<string | null>(null)
const revokingSessionClientId = ref<string | null>(null)
const unblockingClientId = ref<string | null>(null)

const loadClients = async () => {
  loading.value = true
  try {
    const [authorized, blocked, sessionAuths] = await Promise.all([
      getAuthorizedClients(),
      getBlockedClients(),
      getSessionAuthorizations(),
    ])
    authorizedClients.value = authorized
    blockedClients.value = blocked
    sessionAuthorizations.value = sessionAuths
  } catch (error) {
    console.error('Error loading clients:', error)
    add({ description: t('loadError'), color: 'error' })
  } finally {
    loading.value = false
  }
}

const getExtensionName = (extensionId: string): string => {
  const extension = extensionsStore.availableExtensions.find(ext => ext.id === extensionId)
  return extension?.name || extensionId
}

const formatDate = (dateString: string): string => {
  try {
    return new Date(dateString).toLocaleString()
  } catch {
    return dateString
  }
}

const handleRevokeClient = async (client: AuthorizedClient) => {
  revokingClientId.value = client.clientId
  try {
    await revokeClient(client.clientId)
    add({ description: t('revokeSuccess', { name: client.clientName }), color: 'success' })
    await loadClients()
  } catch (error) {
    console.error('Error revoking client:', error)
    add({ description: t('revokeError'), color: 'error' })
  } finally {
    revokingClientId.value = null
  }
}

const handleUnblockClient = async (client: BlockedClient) => {
  unblockingClientId.value = client.clientId
  try {
    await unblockClient(client.clientId)
    add({ description: t('unblockSuccess', { name: client.clientName }), color: 'success' })
    await loadClients()
  } catch (error) {
    console.error('Error unblocking client:', error)
    add({ description: t('unblockError'), color: 'error' })
  } finally {
    unblockingClientId.value = null
  }
}

const handleRevokeSessionAuth = async (auth: SessionAuthorization) => {
  revokingSessionClientId.value = auth.clientId
  try {
    await revokeSessionAuthorization(auth.clientId)
    add({ description: t('revokeSessionSuccess'), color: 'success' })
    await loadClients()
  } catch (error) {
    console.error('Error revoking session authorization:', error)
    add({ description: t('revokeSessionError'), color: 'error' })
  } finally {
    revokingSessionClientId.value = null
  }
}

onMounted(async () => {
  await loadClients()
})
</script>

<i18n lang="yaml">
de:
  title: Externe Clients
  description: Verwalte Browser-Erweiterungen, CLI-Tools und andere externe Anwendungen, die auf deine Vault zugreifen können.
  authorizedClients: Dauerhaft autorisierte Clients
  blockedClients: Blockierte Clients
  sessionAuthorizations: Temporäre Autorisierungen (diese Sitzung)
  noAuthorizedClients: Keine dauerhaft autorisierten Clients vorhanden.
  noBlockedClients: Keine blockierten Clients vorhanden.
  noSessionAuthorizations: Keine temporären Autorisierungen vorhanden.
  authorized: Dauerhaft
  blocked: Blockiert
  sessionOnly: Diese Sitzung
  sessionClient: Externer Client
  sessionHint: Wird beim Neustart von haex-vault entfernt
  extension: Erweiterung
  authorizedAt: Autorisiert am
  blockedAt: Blockiert am
  revoke: Entziehen
  unblock: Entsperren
  loadError: Fehler beim Laden der Clients
  revokeSuccess: 'Zugriff für "{name}" wurde entzogen'
  revokeError: Fehler beim Entziehen des Zugriffs
  revokeSessionSuccess: Temporäre Autorisierung wurde entzogen
  revokeSessionError: Fehler beim Entziehen der temporären Autorisierung
  unblockSuccess: '"{name}" wurde entsperrt'
  unblockError: Fehler beim Entsperren
en:
  title: External Clients
  description: Manage browser extensions, CLI tools, and other external applications that can access your vault.
  authorizedClients: Permanently Authorized Clients
  blockedClients: Blocked Clients
  sessionAuthorizations: Temporary Authorizations (this session)
  noAuthorizedClients: No permanently authorized clients.
  noBlockedClients: No blocked clients.
  noSessionAuthorizations: No temporary authorizations.
  authorized: Permanent
  blocked: Blocked
  sessionOnly: This session
  sessionClient: External Client
  sessionHint: Will be removed when haex-vault restarts
  extension: Extension
  authorizedAt: Authorized at
  blockedAt: Blocked at
  revoke: Revoke
  unblock: Unblock
  loadError: Error loading clients
  revokeSuccess: 'Access for "{name}" has been revoked'
  revokeError: Error revoking access
  revokeSessionSuccess: Temporary authorization has been revoked
  revokeSessionError: Error revoking temporary authorization
  unblockSuccess: '"{name}" has been unblocked'
  unblockError: Error unblocking client
</i18n>
