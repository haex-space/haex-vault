<template>
  <Transition
    :name="direction === 'back' ? 'slide-back' : 'slide-forward'"
    mode="out-in"
  >
    <div
      :key="activeView"
      class="h-full"
    >
      <!-- Subview -->
      <HaexSystemSettingsPeerStorageRelay
        v-if="activeView === 'relay'"
        @back="goBack"
      />
      <HaexSystemSettingsPeerStorageSyncRules
        v-else-if="activeView === 'sync-rules'"
        @back="goBack"
      />

      <!-- Index / Menu -->
      <HaexSystemSettingsLayout
        v-else
        :title="t('title')"
        :description="t('description')"
      >
        <template #description>
          <span
            v-if="store.nodeId"
            class="flex items-center gap-1.5"
          >
            <span class="shrink-0"> {{ t('endpointId') }}: </span>
            <code class="font-mono truncate">{{ store.nodeId }}</code>
            <UButton
              icon="i-lucide-copy"
              color="neutral"
              variant="ghost"
              class="shrink-0"
              @click="copyEndpointId"
            />
          </span>
        </template>

        <template #actions>
          <UiButton
            :icon="store.running ? 'i-lucide-power-off' : 'i-lucide-power'"
            :color="store.running ? 'error' : 'primary'"
            :loading="isToggling"
            @click="onToggleEndpointAsync"
          >
            {{ store.running ? t('actions.stop') : t('actions.start') }}
          </UiButton>
          <div class="basis-full">
            <UCheckbox
              v-model="autostart"
              :label="t('autostart')"
              @update:model-value="onToggleAutostartAsync"
            />
          </div>
        </template>
        <div class="space-y-1">
          <HaexSystemSettingsLayoutMenuItem
            :label="t('menu.relay')"
            :description="t('menu.relayDesc')"
            icon="i-lucide-server"
            @click="navigateTo('relay')"
          />

          <HaexSystemSettingsLayoutMenuItem
            :label="t('menu.syncRules')"
            :description="t('menu.syncRulesDesc')"
            icon="i-lucide-refresh-cw"
            @click="navigateTo('sync-rules')"
          />
        </div>
      </HaexSystemSettingsLayout>
    </div>
  </Transition>
</template>

<script setup lang="ts">
import { and, eq } from 'drizzle-orm'
import { haexVaultSettings } from '~/database/schemas'

const { t } = useI18n()
const store = usePeerStorageStore()
const tabId = inject<string>('haex-tab-id')!
const { activeView, direction, navigateTo, goBack } = useDrillDownNavigation<
  'index' | 'relay' | 'sync-rules'
>('index', 'peer-storage', tabId)

const { copy } = useClipboard()
const { add } = useToast()
const copyEndpointId = async () => {
  await copy(store.nodeId)
  add({ title: t('toast.copied'), color: 'success' })
}

const isToggling = ref(false)
const autostart = ref(false)

const deviceStore = useDeviceStore()

const db = requireDb()

const onToggleAutostartAsync = async (value: boolean | 'indeterminate') => {
  if (value === 'indeterminate') return
  if (!db) return
  if (!deviceStore.deviceId) return

  try {
    const existing = await db.query.haexVaultSettings.findFirst({
      where: and(
        eq(haexVaultSettings.key, VaultSettingsKeyEnum.peerStorageAutostart),
        eq(haexVaultSettings.deviceId, deviceStore.deviceId),
      ),
    })

    if (existing) {
      await db
        .update(haexVaultSettings)
        .set({ value: value ? 'true' : 'false' })
        .where(eq(haexVaultSettings.id, existing.id))
    } else {
      await db.insert(haexVaultSettings).values({
        id: crypto.randomUUID(),
        key: VaultSettingsKeyEnum.peerStorageAutostart,
        deviceId: deviceStore.deviceId,
        value: value ? 'true' : 'false',
      })
    }
  } catch (error) {
    console.error('Failed to save autostart setting:', error)
    add({ description: t('error'), color: 'error' })
  }
}

const onToggleEndpointAsync = async () => {
  isToggling.value = true
  try {
    if (store.running) {
      await store.stopAsync()
      add({ title: t('toast.stopped'), color: 'neutral' })
    } else {
      await store.startAsync()
      add({ title: t('toast.started'), color: 'success' })
    }
  } catch (error) {
    add({
      title: t('error'),
      description: error instanceof Error ? error.message : String(error),
      color: 'error',
    })
  } finally {
    isToggling.value = false
  }
}

onMounted(async () => {
  await store.refreshStatusAsync()
  if (db && deviceStore.deviceId) {
    const row = await db.query.haexVaultSettings.findFirst({
      where: and(
        eq(haexVaultSettings.key, VaultSettingsKeyEnum.peerStorageAutostart),
        eq(haexVaultSettings.deviceId, deviceStore.deviceId),
      ),
    })
    // Default-on: only explicit 'false' disables autostart.
    autostart.value = row?.value !== 'false'
  }
})
</script>

<i18n lang="yaml">
de:
  title: P2P Netzwerk
  description: Verbindung zu anderen Peers über ein verschlüsseltes P2P-Netzwerk
  endpointId: Endpoint-ID
  autostart: Automatisch starten wenn die Vault geöffnet wird
  actions:
    start: Start
    stop: Stop
  menu:
    relay: Relay-Server
    relayDesc: Relay für NAT-Traversal konfigurieren
    syncRules: Sync-Regeln
    syncRulesDesc: Dateien automatisch zwischen Geräten synchronisieren
  toast:
    copied: Endpoint-ID kopiert
    started: P2P-Endpoint gestartet
    stopped: P2P-Endpoint gestoppt

en:
  title: P2P Network
  description: Connection to other peers over an encrypted P2P network
  endpointId: Endpoint-ID
  autostart: Automatically start when the vault is opened
  actions:
    start: Start
    stop: Stop
  menu:
    relay: Relay Server
    relayDesc: Configure relay for NAT traversal
    syncRules: Sync Rules
    syncRulesDesc: Automatically synchronize files between devices
  toast:
    copied: Endpoint ID copied
    started: P2P endpoint started
    stopped: P2P endpoint stopped
</i18n>
