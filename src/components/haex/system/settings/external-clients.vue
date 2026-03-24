<template>
  <HaexSystemSettingsLayout :title="t('title')" :description="t('description')">
    <!-- Bridge Configuration -->
    <HaexSystemSettingsLayoutSection
      :title="t('bridgeConfigTitle')"
      :description="t('bridgeConfigPortHint')"
      default-open
    >
      <template #actions>
        <UButton
          :label="t('bridgeConfigApply')"
          :loading="savingPort"
          :disabled="bridgePort === currentPort || !isValidPort"
          @click="handleSavePort"
        />
      </template>

      <div class="space-y-4">
        <div class="flex flex-col @sm:flex-row @sm:items-center gap-3">
          <label class="text-sm font-medium flex-1">{{ t('bridgeConfigPort') }}</label>
          <UInput
            v-model.number="bridgePort"
            type="number"
            :min="1024"
            :max="65535"
            class="w-28"
            :disabled="savingPort"
          />
        </div>

        <div class="flex items-center gap-2 text-sm">
          <UIcon
            :name="bridgeRunning ? 'i-heroicons-check-circle' : 'i-heroicons-x-circle'"
            :class="bridgeRunning ? 'text-success' : 'text-error'"
            class="w-4 h-4"
          />
          <span v-if="bridgeRunning">
            {{ t('bridgeConfigRunning', { port: currentPort }) }}
          </span>
          <span v-else>
            {{ t('bridgeConfigStopped') }}
          </span>
        </div>
      </div>
    </HaexSystemSettingsLayoutSection>

    <div v-if="loading" class="flex justify-center py-8">
      <UIcon
        name="i-heroicons-arrow-path"
        class="w-8 h-8 animate-spin text-primary"
      />
    </div>

    <template v-else>
      <!-- Authorized Clients -->
      <HaexSystemSettingsLayoutSection
        :title="t('authorizedClients')"
      >
        <HaexSystemSettingsLayoutEmpty
          v-if="!authorizedClients.length"
          :message="t('noAuthorizedClients')"
        />

        <UiListContainer v-else>
          <UiListItem
            v-for="client in authorizedClients"
            :key="client.id"
          >
            <div class="flex items-center gap-2">
              <span class="font-semibold">{{ client.clientName }}</span>
              <UBadge color="success" variant="subtle">
                {{ t('authorized') }}
              </UBadge>
            </div>
            <div class="text-sm text-muted mt-1">
              {{ t('extension') }}: {{ getExtensionName(client.extensionId) }}
            </div>
            <div class="text-xs text-muted mt-1 font-mono truncate">
              {{ client.clientId }}
            </div>
            <div v-if="client.authorizedAt" class="text-xs text-muted mt-1">
              {{ t('authorizedAt') }}: {{ formatDate(client.authorizedAt) }}
            </div>
            <template #actions>
              <UButton
                color="error"
                variant="ghost"
                :loading="revokingClientId === client.clientId"
                @click="handleRevokeClient(client)"
              >
                <UIcon name="i-heroicons-x-mark" class="w-4 h-4" />
                {{ t('revoke') }}
              </UButton>
            </template>
          </UiListItem>
        </UiListContainer>
      </HaexSystemSettingsLayoutSection>

      <!-- Session Decisions -->
      <HaexSystemSettingsLayoutSection
        :title="t('sessionDecisions')"
      >
        <HaexSystemSettingsLayoutEmpty
          v-if="!sessionAuthorizations.length && !sessionBlockedClients.length"
          :message="t('noSessionDecisions')"
        />

        <UiListContainer v-else>
          <UiListItem
            v-for="auth in sessionAuthorizations"
            :key="'auth-' + auth.clientId"
          >
            <div class="flex items-center gap-2">
              <span class="font-semibold">{{ t('sessionClient') }}</span>
              <UBadge color="success" variant="subtle">
                {{ t('sessionAllowed') }}
              </UBadge>
            </div>
            <div class="text-sm text-muted mt-1">
              {{ t('extension') }}: {{ getExtensionName(auth.extensionId) }}
            </div>
            <div class="text-xs text-muted mt-1 font-mono truncate">
              {{ auth.clientId }}
            </div>
            <div class="text-xs text-muted mt-1">
              {{ t('sessionHint') }}
            </div>
            <template #actions>
              <UButton
                color="error"
                variant="ghost"
                :loading="revokingSessionClientId === auth.clientId"
                @click="handleRevokeSessionAuth(auth)"
              >
                <UIcon name="i-heroicons-x-mark" class="w-4 h-4" />
                {{ t('revoke') }}
              </UButton>
            </template>
          </UiListItem>

          <UiListItem
            v-for="client in sessionBlockedClients"
            :key="'blocked-' + client.clientId"
          >
            <div class="flex items-center gap-2">
              <span class="font-semibold">{{ client.clientName }}</span>
              <UBadge color="error" variant="subtle">
                {{ t('sessionBlocked') }}
              </UBadge>
            </div>
            <div class="text-xs text-muted mt-1 font-mono truncate">
              {{ client.clientId }}
            </div>
            <div class="text-xs text-muted mt-1">
              {{ t('sessionHint') }}
            </div>
            <template #actions>
              <UButton
                color="success"
                variant="ghost"
                :loading="unblockingSessionClientId === client.clientId"
                @click="handleUnblockSessionClient(client)"
              >
                <UIcon name="i-heroicons-check" class="w-4 h-4" />
                {{ t('unblock') }}
              </UButton>
            </template>
          </UiListItem>
        </UiListContainer>
      </HaexSystemSettingsLayoutSection>

      <!-- Blocked Clients -->
      <HaexSystemSettingsLayoutSection
        :title="t('blockedClients')"
      >
        <HaexSystemSettingsLayoutEmpty
          v-if="!blockedClients.length"
          :message="t('noBlockedClients')"
        />

        <UiListContainer v-else>
          <UiListItem
            v-for="client in blockedClients"
            :key="client.id"
          >
            <div class="flex items-center gap-2">
              <span class="font-semibold">{{ client.clientName }}</span>
              <UBadge color="error" variant="subtle">
                {{ t('blocked') }}
              </UBadge>
            </div>
            <div class="text-xs text-muted mt-1 font-mono truncate">
              {{ client.clientId }}
            </div>
            <div v-if="client.blockedAt" class="text-xs text-muted mt-1">
              {{ t('blockedAt') }}: {{ formatDate(client.blockedAt) }}
            </div>
            <template #actions>
              <UButton
                color="success"
                variant="ghost"
                :loading="unblockingClientId === client.clientId"
                @click="handleUnblockClient(client)"
              >
                <UIcon name="i-heroicons-check" class="w-4 h-4" />
                {{ t('unblock') }}
              </UButton>
            </template>
          </UiListItem>
        </UiListContainer>
      </HaexSystemSettingsLayoutSection>
    </template>
  </HaexSystemSettingsLayout>
</template>

<script setup lang="ts">
import { invoke } from '@tauri-apps/api/core'
import type { AuthorizedClient, BlockedClient, SessionAuthorization } from '@haex-space/vault-sdk'
import type { SessionBlockedClient } from '~~/src-tauri/bindings/SessionBlockedClient'

const { t } = useI18n()
const { add } = useToast()
const { getAuthorizedClients, getBlockedClients, revokeClient, unblockClient, getSessionAuthorizations, revokeSessionAuthorization, getSessionBlockedClients, unblockSessionClient } = useExternalAuth()
const extensionsStore = useExtensionsStore()
const vaultSettingsStore = useVaultSettingsStore()

const loading = ref(true)
const authorizedClients = ref<AuthorizedClient[]>([])
const blockedClients = ref<BlockedClient[]>([])
const sessionAuthorizations = ref<SessionAuthorization[]>([])
const sessionBlockedClients = ref<SessionBlockedClient[]>([])
const revokingClientId = ref<string | null>(null)
const revokingSessionClientId = ref<string | null>(null)
const unblockingClientId = ref<string | null>(null)
const unblockingSessionClientId = ref<string | null>(null)

// Bridge port configuration
const bridgePort = ref(19455)
const currentPort = ref(19455)
const bridgeRunning = ref(false)
const savingPort = ref(false)

const isValidPort = computed(() => {
  return bridgePort.value >= 1024 && bridgePort.value <= 65535
})

const loadBridgeStatus = async () => {
  try {
    const [running, port, savedPort] = await Promise.all([
      invoke<boolean>('external_bridge_get_status'),
      invoke<number>('external_bridge_get_port'),
      vaultSettingsStore.getExternalBridgePortAsync(),
    ])
    bridgeRunning.value = running
    currentPort.value = port
    bridgePort.value = savedPort
  } catch (error) {
    console.error('Error loading bridge status:', error)
  }
}

const handleSavePort = async () => {
  if (!isValidPort.value || bridgePort.value === currentPort.value) return

  savingPort.value = true
  try {
    // Save to database
    const newPort = await vaultSettingsStore.updateExternalBridgePortAsync(bridgePort.value)

    // Restart bridge with new port
    await invoke('external_bridge_stop')
    await invoke('external_bridge_start', { port: newPort })

    currentPort.value = newPort
    bridgeRunning.value = true

    add({ description: t('bridgeConfigPortChanged', { port: newPort }), color: 'success' })
  } catch (error) {
    console.error('Error changing bridge port:', error)
    add({ description: t('bridgeConfigPortChangeError'), color: 'error' })
  } finally {
    savingPort.value = false
  }
}

const loadClients = async () => {
  loading.value = true
  try {
    const [authorized, blocked, sessionAuths, sessionBlocked] = await Promise.all([
      getAuthorizedClients(),
      getBlockedClients(),
      getSessionAuthorizations(),
      getSessionBlockedClients(),
    ])
    authorizedClients.value = authorized
    blockedClients.value = blocked
    sessionAuthorizations.value = sessionAuths
    sessionBlockedClients.value = sessionBlocked
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

const handleUnblockSessionClient = async (client: SessionBlockedClient) => {
  unblockingSessionClientId.value = client.clientId
  try {
    await unblockSessionClient(client.clientId)
    add({ description: t('unblockSessionSuccess', { name: client.clientName }), color: 'success' })
    await loadClients()
  } catch (error) {
    console.error('Error unblocking session client:', error)
    add({ description: t('unblockSessionError'), color: 'error' })
  } finally {
    unblockingSessionClientId.value = null
  }
}

// Listen for authorization decisions to refresh the list
const { decisionCounter } = useExternalAuth()
watch(decisionCounter, () => {
  loadClients()
})

onMounted(async () => {
  await Promise.all([
    loadClients(),
    loadBridgeStatus(),
  ])
})
</script>

<i18n lang="yaml">
de:
  title: Externe Clients
  description: Verwalte Browser-Erweiterungen, CLI-Tools und andere externe Anwendungen, die auf deine Vault zugreifen können.
  bridgeConfigTitle: Bridge-Konfiguration
  bridgeConfigPort: Port
  bridgeConfigPortHint: "Port für die WebSocket-Verbindung externer Clients (Standard: 19455)"
  bridgeConfigApply: Anwenden
  bridgeConfigRunning: 'Bridge läuft auf Port {port}'
  bridgeConfigStopped: Bridge ist gestoppt
  bridgeConfigPortChanged: 'Port wurde auf {port} geändert'
  bridgeConfigPortChangeError: Fehler beim Ändern des Ports
  authorizedClients: Dauerhaft autorisierte Clients
  blockedClients: Dauerhaft blockierte Clients
  sessionDecisions: Temporäre Entscheidungen (diese Sitzung)
  noAuthorizedClients: Keine dauerhaft autorisierten Clients vorhanden.
  noBlockedClients: Keine dauerhaft blockierten Clients vorhanden.
  noSessionDecisions: Keine temporären Entscheidungen vorhanden.
  authorized: Dauerhaft
  blocked: Dauerhaft
  sessionAllowed: Erlaubt
  sessionBlocked: Blockiert
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
  unblockSessionSuccess: '"{name}" wurde für diese Sitzung entsperrt'
  unblockSessionError: Fehler beim Entsperren
en:
  title: External Clients
  description: Manage browser extensions, CLI tools, and other external applications that can access your vault.
  bridgeConfigTitle: Bridge Configuration
  bridgeConfigPort: Port
  bridgeConfigPortHint: "Port for WebSocket connections from external clients (default: 19455)"
  bridgeConfigApply: Apply
  bridgeConfigRunning: 'Bridge running on port {port}'
  bridgeConfigStopped: Bridge is stopped
  bridgeConfigPortChanged: 'Port changed to {port}'
  bridgeConfigPortChangeError: Error changing port
  authorizedClients: Permanently Authorized Clients
  blockedClients: Permanently Blocked Clients
  sessionDecisions: Temporary Decisions (this session)
  noAuthorizedClients: No permanently authorized clients.
  noBlockedClients: No permanently blocked clients.
  noSessionDecisions: No temporary decisions.
  authorized: Permanent
  blocked: Permanent
  sessionAllowed: Allowed
  sessionBlocked: Blocked
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
  unblockSessionSuccess: '"{name}" has been unblocked for this session'
  unblockSessionError: Error unblocking client
</i18n>
