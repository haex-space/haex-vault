<template>
  <UiDrawerModal
    v-model:open="open"
    :title="isEditMode ? t('titleEdit') : t('title')"
    :description="t('description')"
  >
    <template #body>
      <UStepper
        v-model="wizard.step.value"
        :items="stepperItems"
        orientation="horizontal"
        class="mb-6"
      >
        <!-- Step 1: Source -->
        <template #source>
          <HaexSystemSettingsPeerStorageCreateSyncRuleDialogStepSource
            v-model:type="wizard.source.type.value"
            v-model:path="wizard.source.path.value"
            v-model:space-id="wizard.source.spaceId.value"
            v-model:device-endpoint-id="wizard.source.deviceEndpointId.value"
            v-model:share-id="wizard.source.shareId.value"
            v-model:subfolder="wizard.source.subfolder.value"
            v-model:backend-id="wizard.source.backendId.value"
            v-model:bucket="wizard.source.bucket.value"
            v-model:prefix="wizard.source.prefix.value"
            :provider-types="sourceProviderTypes"
            :space-options="spaceOptions"
            :backend-options="backendOptions"
            :device-options-for-space="deviceOptionsForSpace"
            :share-options-for-device="shareOptionsForDevice"
            :default-bucket-for="defaultBucketFor"
            @select-folder="selectSourceFolderAsync"
          />
        </template>

        <!-- Step 2: Target -->
        <template #target>
          <HaexSystemSettingsPeerStorageCreateSyncRuleDialogStepTarget
            v-model:type="wizard.target.type.value"
            v-model:path="wizard.target.path.value"
            v-model:space-id="wizard.target.spaceId.value"
            v-model:device-endpoint-id="wizard.target.deviceEndpointId.value"
            v-model:share-id="wizard.target.shareId.value"
            v-model:create-new-folder="wizard.target.createNewFolder.value"
            v-model:new-folder-name="wizard.target.newFolderName.value"
            v-model:subfolder="wizard.target.subfolder.value"
            v-model:backend-id="wizard.target.backendId.value"
            v-model:bucket="wizard.target.bucket.value"
            v-model:prefix="wizard.target.prefix.value"
            :provider-types="targetProviderTypes"
            :space-options="spaceOptions"
            :backend-options="backendOptions"
            :device-options-for-space="deviceOptionsForSpace"
            :share-options-for-device="shareOptionsForDevice"
            :default-bucket-for="defaultBucketFor"
            @select-folder="selectTargetFolderAsync"
          />
        </template>

        <!-- Step 3: Settings -->
        <template #settings>
          <HaexSystemSettingsPeerStorageCreateSyncRuleDialogStepSettings
            v-model:direction="wizard.settings.direction.value"
            v-model:delete-mode="wizard.settings.deleteMode.value"
            :delete-mode-options="deleteModeOptions"
          />
        </template>
      </UStepper>
    </template>

    <template #footer>
      <div class="flex justify-between gap-4">
        <div class="flex gap-2">
          <UiButton
            color="neutral"
            variant="outline"
            @click="onBack"
          >
            {{ wizard.step.value > 0 ? t('actions.back') : t('actions.cancel') }}
          </UiButton>
          <template v-if="isEditMode && editRule">
            <UiButton
              :icon="editRule.enabled ? 'i-lucide-pause' : 'i-lucide-play'"
              variant="outline"
              :color="editRule.enabled ? 'warning' : 'success'"
              @click="onToggleRuleAsync"
            />
            <UiButton
              icon="i-lucide-trash-2"
              variant="outline"
              color="error"
              @click="onDeleteRuleAsync"
            />
          </template>
        </div>

        <UiButton
          v-if="wizard.step.value < 2"
          icon="i-lucide-arrow-right"
          :disabled="!wizard.canProceed.value"
          @click="() => { wizard.step.value++ }"
        >
          {{ t('actions.next') }}
        </UiButton>
        <UiButton
          v-else
          icon="i-lucide-check"
          color="primary"
          :loading="isCreating"
          :disabled="!wizard.canCreate.value"
          @click="onSaveAsync"
        >
          {{ isEditMode ? t('actions.save') : t('actions.create') }}
        </UiButton>
      </div>
    </template>
  </UiDrawerModal>
</template>

<script setup lang="ts">
import type { StepperItem } from '@nuxt/ui'
import { invoke } from '@tauri-apps/api/core'
import type { SelectHaexSyncRules } from '~/database/schemas'
import { getUcanForSpaceAsync } from '~/utils/auth/ucanStore'
import type { StorageBackendInfo } from '~/../src-tauri/bindings/StorageBackendInfo'
import { useCreateSyncRuleWizard, type ProviderType } from '~/composables/peerStorage/useCreateSyncRuleWizard'

const open = defineModel<boolean>('open', { required: true })

const props = defineProps<{
  editRule?: SelectHaexSyncRules | null
  prefill?: {
    sourceType: 'local' | 'peer'
    spaceId: string
    endpointId: string
    shareName: string
    localPath?: string
  } | null
}>()

const emit = defineEmits<{
  created: []
  updated: []
  deleted: []
}>()

const isEditMode = computed(() => !!props.editRule)

const { t } = useI18n()
const { add: addToast } = useToast()
const fileSyncStore = useFileSyncStore()
const spacesStore = useSpacesStore()
const peerStorageStore = usePeerStorageStore()
const deviceStore = useDeviceStore()

const storageBackends = ref<StorageBackendInfo[]>([])

const loadStorageBackendsAsync = async () => {
  try {
    storageBackends.value = await invoke<StorageBackendInfo[]>('remote_storage_list_backends')
  } catch (error) {
    addToast({
      title: t('errors.loadBackendsFailed'),
      description: error instanceof Error ? error.message : String(error),
      color: 'error',
    })
  }
}

const wizard = useCreateSyncRuleWizard()
const isCreating = ref(false)

// -- Stepper items --
const stepperItems = computed<StepperItem[]>(() => [
  {
    slot: 'source',
    title: t('steps.source'),
    icon: 'i-lucide-upload',
  },
  {
    slot: 'target',
    title: t('steps.target'),
    icon: 'i-lucide-download',
  },
  {
    slot: 'settings',
    title: t('steps.settings'),
    icon: 'i-lucide-settings',
  },
])

// -- Provider types --
// All providers available as source
const sourceProviderTypes = computed(() => [
  { value: 'local' as ProviderType, label: t('provider.local'), icon: 'i-lucide-folder' },
  { value: 'peer' as ProviderType, label: t('provider.peer'), icon: 'i-lucide-monitor-smartphone' },
  { value: 'cloud' as ProviderType, label: t('provider.cloud'), icon: 'i-lucide-cloud' },
])

const targetProviderTypes = computed(() => [
  { value: 'local' as ProviderType, label: t('provider.local'), icon: 'i-lucide-folder' },
  { value: 'peer' as ProviderType, label: t('provider.peer'), icon: 'i-lucide-monitor-smartphone' },
  { value: 'cloud' as ProviderType, label: t('provider.cloud'), icon: 'i-lucide-cloud' },
])

// -- Options --
const deleteModeOptions = computed(() => [
  { label: t('deleteModes.trash'), value: 'trash' },
  { label: t('deleteModes.permanent'), value: 'permanent' },
  { label: t('deleteModes.ignore'), value: 'ignore' },
])

const spaceOptions = computed(() =>
  spacesStore.activeSpaces.map(s => ({ label: s.name, value: s.id })),
)

const backendOptions = computed(() =>
  storageBackends.value
    .filter(b => b.enabled)
    .map(b => ({ label: b.name, value: b.id })),
)

const defaultBucketFor = (backendId: string): string => {
  const backend = storageBackends.value.find(b => b.id === backendId)
  return backend?.config?.bucket || ''
}

const deviceOptionsForSpace = (spaceId: string) =>
  peerStorageStore.spaceDevices
    .filter(d => d.spaceId === spaceId)
    .map(d => ({ label: d.name, value: d.endpointId }))

const shareOptionsForDevice = (endpointId: string) =>
  peerStorageStore.shares
    .filter(s => s.endpointId === endpointId)
    .map(s => ({ label: s.name, value: s.id }))

// -- Folder selection --
const selectSourceFolderAsync = async () => {
  const path = await invoke<string | null>('filesystem_select_folder', {})
  if (path) wizard.source.path.value = path
}

const selectTargetFolderAsync = async () => {
  const path = await invoke<string | null>('filesystem_select_folder', {})
  if (path) wizard.target.path.value = path
}

// -- Build config objects --
const buildSourceConfig = () => {
  switch (wizard.source.type.value) {
    case 'local': return { path: wizard.source.path.value }
    case 'peer': {
      const spaceId = wizard.source.spaceId.value
      const ucanToken = spaceId ? getUcanForSpaceAsync(spaceId) : null
      if (!ucanToken) throw new Error('No valid UCAN token for this space')
      const basePath = wizard.source.shareId.value
      const sub = wizard.source.subfolder.value.trim().replace(/^\/+|\/+$/g, '')
      const path = sub ? `${basePath}/${sub}` : basePath
      return {
        endpointId: wizard.source.deviceEndpointId.value,
        path,
        spaceId,
        ucanToken,
      }
    }
    case 'cloud': return {
      backendId: wizard.source.backendId.value,
      // Only send an override when the user actually changed it
      bucket: wizard.source.bucket.value.trim() || undefined,
      prefix: wizard.source.prefix.value,
    }
  }
}

const buildTargetConfig = () => {
  switch (wizard.target.type.value) {
    case 'local': return { path: wizard.target.path.value }
    case 'peer': {
      const spaceId = wizard.target.spaceId.value
      const ucanToken = spaceId ? getUcanForSpaceAsync(spaceId) : null
      if (!ucanToken) throw new Error('No valid UCAN token for this space')
      const basePath = wizard.target.createNewFolder.value
        ? wizard.target.newFolderName.value.trim()
        : wizard.target.shareId.value
      const sub = wizard.target.subfolder.value.trim().replace(/^\/+|\/+$/g, '')
      const path = sub ? `${basePath}/${sub}` : basePath
      return {
        endpointId: wizard.target.deviceEndpointId.value,
        path,
        spaceId,
        ucanToken,
      }
    }
    case 'cloud': return {
      backendId: wizard.target.backendId.value,
      bucket: wizard.target.bucket.value.trim() || undefined,
      prefix: wizard.target.prefix.value,
    }
  }
}

// -- Resolve current device DB id --
//
// The row in haex_devices is created up-front when the vault is opened (see
// useDeviceStore.resolveAsync / registerNewAsync). Sync rules just reference
// that row id as a FK; we do not lazy-create here anymore.
const resolveCurrentDeviceId = (): string => {
  const id = deviceStore.deviceRowId
  if (!id) throw new Error('Device identity not resolved — open the vault first')
  return id
}

// -- Determine spaceId for the rule --
const resolveSpaceId = (): string => {
  if (wizard.source.type.value === 'peer' && wizard.source.spaceId.value) return wizard.source.spaceId.value
  if (wizard.target.type.value === 'peer' && wizard.target.spaceId.value) return wizard.target.spaceId.value
  return spacesStore.visibleSpaces[0]?.id ?? ''
}

// -- Navigation --
const onBack = () => {
  if (wizard.step.value > 0) {
    wizard.step.value--
  } else {
    open.value = false
  }
}

// -- Toggle / Delete rule --
const onToggleRuleAsync = async () => {
  if (!props.editRule) return
  try {
    await fileSyncStore.toggleRuleAsync(props.editRule.id, !props.editRule.enabled)
    addToast({ title: props.editRule.enabled ? t('success.paused') : t('success.resumed'), color: 'success' })
    open.value = false
    emit('updated')
  } catch (error) {
    addToast({ title: t('errors.createFailed'), description: error instanceof Error ? error.message : String(error), color: 'error' })
  }
}

const onDeleteRuleAsync = async () => {
  if (!props.editRule) return
  try {
    await fileSyncStore.deleteRuleAsync(props.editRule.id)
    addToast({ title: t('success.deleted'), color: 'success' })
    open.value = false
    emit('deleted')
  } catch (error) {
    addToast({ title: t('errors.createFailed'), description: error instanceof Error ? error.message : String(error), color: 'error' })
  }
}

// -- Save rule (create or update) --
const onSaveAsync = async () => {
  if (!wizard.canCreate.value) return
  isCreating.value = true

  try {
    if (isEditMode.value && props.editRule) {
      await fileSyncStore.updateRuleAsync(props.editRule.id, {
        sourceType: wizard.source.type.value,
        sourceConfig: buildSourceConfig(),
        targetType: wizard.target.type.value,
        targetConfig: buildTargetConfig(),
        direction: wizard.settings.direction.value,
        syncIntervalSeconds: wizard.settings.intervalSeconds.value,
        deleteMode: wizard.settings.deleteMode.value,
      })
      addToast({ title: t('success.updated'), color: 'success' })
      emit('updated')
    } else {
      const deviceId = resolveCurrentDeviceId()
      const spaceId = resolveSpaceId()
      if (!spaceId) throw new Error('No space available')

      await fileSyncStore.createRuleAsync({
        id: crypto.randomUUID(),
        spaceId,
        deviceId,
        sourceType: wizard.source.type.value,
        sourceConfig: buildSourceConfig(),
        targetType: wizard.target.type.value,
        targetConfig: buildTargetConfig(),
        direction: wizard.settings.direction.value,
        syncIntervalSeconds: wizard.settings.intervalSeconds.value,
        deleteMode: wizard.settings.deleteMode.value,
        enabled: true,
      })
      addToast({ title: t('success.created'), color: 'success' })
      emit('created')
    }

    open.value = false
  } catch (error) {
    addToast({
      title: t('errors.createFailed'),
      description: error instanceof Error ? error.message : String(error),
      color: 'error',
    })
  } finally {
    isCreating.value = false
  }
}

const normalizeConfig = (cfg: unknown): Record<string, unknown> => {
  if (typeof cfg === 'string') {
    try {
      return JSON.parse(cfg) as Record<string, unknown>
    } catch {
      return {}
    }
  }
  return cfg && typeof cfg === 'object' ? (cfg as Record<string, unknown>) : {}
}

const populateFromRule = (rule: SelectHaexSyncRules) => {
  const srcCfg = normalizeConfig(rule.sourceConfig)
  const tgtCfg = normalizeConfig(rule.targetConfig)

  wizard.source.type.value = rule.sourceType as ProviderType
  wizard.target.type.value = rule.targetType as ProviderType
  wizard.settings.direction.value = rule.direction as 'one_way' | 'two_way'
  wizard.settings.intervalSeconds.value = rule.syncIntervalSeconds
  wizard.settings.deleteMode.value = rule.deleteMode

  // Source
  if (rule.sourceType === 'local') {
    wizard.source.path.value = (srcCfg?.path as string) || ''
  } else if (rule.sourceType === 'peer') {
    wizard.source.spaceId.value = (srcCfg?.spaceId as string) || ''
    wizard.source.deviceEndpointId.value = (srcCfg?.endpointId as string) || ''
    wizard.source.shareId.value = (srcCfg?.path as string) || ''
  } else if (rule.sourceType === 'cloud') {
    wizard.source.backendId.value = (srcCfg?.backendId as string) || ''
    wizard.source.bucket.value = (srcCfg?.bucket as string) || ''
    wizard.source.prefix.value = (srcCfg?.prefix as string) || ''
  }

  // Target
  if (rule.targetType === 'local') {
    wizard.target.path.value = (tgtCfg?.path as string) || ''
  } else if (rule.targetType === 'peer') {
    wizard.target.spaceId.value = (tgtCfg?.spaceId as string) || ''
    wizard.target.deviceEndpointId.value = (tgtCfg?.endpointId as string) || ''
    wizard.target.shareId.value = (tgtCfg?.path as string) || ''
  } else if (rule.targetType === 'cloud') {
    wizard.target.backendId.value = (tgtCfg?.backendId as string) || ''
    wizard.target.bucket.value = (tgtCfg?.bucket as string) || ''
    wizard.target.prefix.value = (tgtCfg?.prefix as string) || ''
  }
}

watch(open, async (isOpen) => {
  if (isOpen) {
    wizard.reset()
    await peerStorageStore.loadSharesAsync()
    await peerStorageStore.loadSpaceDevicesAsync()
    await loadStorageBackendsAsync()
    if (props.editRule) {
      populateFromRule(props.editRule)
    } else if (props.prefill) {
      wizard.source.type.value = props.prefill.sourceType
      if (props.prefill.sourceType === 'local' && props.prefill.localPath) {
        wizard.source.path.value = props.prefill.localPath
      } else {
        wizard.source.spaceId.value = props.prefill.spaceId
        wizard.source.deviceEndpointId.value = props.prefill.endpointId
        wizard.source.shareId.value = props.prefill.shareName
      }
      wizard.step.value = 1 // Jump to target step
    }
  }
})
</script>

<i18n lang="yaml">
de:
  title: Sync-Regel erstellen
  titleEdit: Sync-Regel bearbeiten
  description: Dateien automatisch zwischen Quell- und Zielordner synchronisieren
  steps:
    source: Quelle
    sourceDescription: Woher kommen die Daten
    target: Ziel
    targetDescription: Wohin sollen sie synchronisiert werden
    settings: Einstellungen
    settingsDescription: Intervall, Richtung und Verhalten
  provider:
    local: Lokaler Ordner
    peer: P2P Peer
    cloud: Cloud-Speicher
  source:
    type: Quelltyp
    selectFolder: Ordner auswählen
    space: Space
    device: Gerät
    share: Freigabe
    subfolder: Unterordner (optional)
    subfolderPlaceholder: z.B. Bilder/Urlaub
    backend: Storage-Backend
    bucket: Bucket
    bucketDescription: Leer lassen um den Bucket des Backends zu verwenden. Wird automatisch angelegt, falls nicht vorhanden.
    prefix: Pfad-Präfix
  target:
    type: Zieltyp
    selectFolder: Ordner auswählen
    space: Space
    device: Gerät
    folder: Zielordner
    createNew: Neuen Ordner erstellen
    chooseExisting: Vorhandenen wählen
    newFolderPlaceholder: Ordnername eingeben
    subfolder: Unterordner (optional)
    subfolderPlaceholder: z.B. Backup/Fotos
    backend: Storage-Backend
    bucket: Bucket
    bucketDescription: Leer lassen um den Bucket des Backends zu verwenden. Wird automatisch angelegt, falls nicht vorhanden.
    prefix: Pfad-Präfix
  settings:
    direction: Richtung
    oneWay: Einseitig
    twoWay: Beidseitig
    interval: Sync-Intervall
    deleteMode: Löschmodus
    name: Regelname
    namePlaceholder: z.B. Fotos-Backup
  intervals:
    1min: Jede Minute
    5min: Alle 5 Minuten
    15min: Alle 15 Minuten
    30min: Alle 30 Minuten
    1hour: Stündlich
    manual: Nur manuell
  deleteModes:
    trash: In Papierkorb verschieben
    permanent: Endgültig löschen
    ignore: Löschungen ignorieren
  actions:
    cancel: Abbrechen
    back: Zurück
    next: Weiter
    create: Erstellen
    save: Speichern
    pause: Pausieren
    resume: Fortsetzen
    delete: Löschen
  success:
    created: Sync-Regel erstellt
    updated: Sync-Regel aktualisiert
    paused: Sync-Regel pausiert
    resumed: Sync-Regel fortgesetzt
    deleted: Sync-Regel gelöscht
  errors:
    createFailed: Sync-Regel konnte nicht erstellt werden
    loadBackendsFailed: Storage-Backends konnten nicht geladen werden
en:
  title: Create Sync Rule
  titleEdit: Edit Sync Rule
  description: Automatically synchronize files between source and target
  steps:
    source: Source
    sourceDescription: Where the data comes from
    target: Target
    targetDescription: Where to synchronize it to
    settings: Settings
    settingsDescription: Interval, direction and behavior
  provider:
    local: Local Folder
    peer: P2P Peer
    cloud: Cloud Storage
  source:
    type: Source type
    selectFolder: Select folder
    space: Space
    device: Device
    share: Share
    subfolder: Subfolder (optional)
    subfolderPlaceholder: e.g. Pictures/Vacation
    backend: Storage backend
    bucket: Bucket
    bucketDescription: Leave empty to use the backend's default bucket. Created automatically if missing.
    prefix: Path prefix
  target:
    type: Target type
    selectFolder: Select folder
    space: Space
    device: Device
    folder: Target folder
    createNew: Create new folder
    chooseExisting: Choose existing
    newFolderPlaceholder: Enter folder name
    subfolder: Subfolder (optional)
    subfolderPlaceholder: e.g. Backup/Photos
    backend: Storage backend
    bucket: Bucket
    bucketDescription: Leave empty to use the backend's default bucket. Created automatically if missing.
    prefix: Path prefix
  settings:
    direction: Direction
    oneWay: One-way
    twoWay: Two-way
    interval: Sync interval
    deleteMode: Delete mode
    name: Rule name
    namePlaceholder: e.g. Photos Backup
  intervals:
    1min: Every minute
    5min: Every 5 minutes
    15min: Every 15 minutes
    30min: Every 30 minutes
    1hour: Hourly
    manual: Manual only
  deleteModes:
    trash: Move to trash
    permanent: Delete permanently
    ignore: Ignore deletions
  actions:
    cancel: Cancel
    back: Back
    next: Next
    create: Create
    save: Save
    pause: Pause
    resume: Resume
    delete: Delete
  success:
    created: Sync rule created
    updated: Sync rule updated
    paused: Sync rule paused
    resumed: Sync rule resumed
    deleted: Sync rule deleted
  errors:
    createFailed: Failed to save sync rule
    loadBackendsFailed: Failed to load storage backends
</i18n>
