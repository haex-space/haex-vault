<template>
  <UiDrawerModal
    v-model:open="open"
    :title="isEditMode ? t('titleEdit') : t('title')"
    :description="t('description')"
  >
    <template #body>
      <UStepper
        v-model="step"
        :items="stepperItems"
        orientation="horizontal"
        class="mb-6"
      >
        <!-- Step 1: Source -->
        <template #source>
          <div class="space-y-4 pt-4">
            <USelectMenu
              v-model="sourceType"
              :items="sourceProviderTypes"
              value-key="value"
              :label="t('source.type')"
              class="w-full"
            />

            <!-- Local folder picker -->
            <div v-if="sourceType === 'local'" class="space-y-2">
              <UButton
                icon="i-lucide-folder"
                color="neutral"
                variant="outline"
                block
                @click="selectSourceFolderAsync"
              >
                {{ sourcePath || t('source.selectFolder') }}
              </UButton>
            </div>

            <!-- Peer: space + device + share pickers -->
            <div v-if="sourceType === 'peer'" class="space-y-3">
              <UiSelectMenu
                v-model="sourceSpaceId"
                :items="spaceOptions"
                :label="t('source.space')"
                value-key="value"
              />
              <UiSelectMenu
                v-if="sourceSpaceId"
                v-model="sourceDeviceEndpointId"
                :items="deviceOptionsForSpace(sourceSpaceId)"
                :label="t('source.device')"
                value-key="value"
              />
              <UiSelectMenu
                v-if="sourceDeviceEndpointId"
                v-model="sourceShareId"
                :items="shareOptionsForDevice(sourceDeviceEndpointId)"
                :label="t('source.share')"
                value-key="value"
              />
              <UiInput
                v-if="sourceShareId"
                v-model="sourceSubfolder"
                :label="t('source.subfolder')"
                :placeholder="t('source.subfolderPlaceholder')"
              />
            </div>

            <!-- Cloud: backend + prefix -->
            <div v-if="sourceType === 'cloud'" class="space-y-3">
              <UiSelectMenu
                v-model="sourceBackendId"
                :items="backendOptions"
                :label="t('source.backend')"
                value-key="value"
              />
              <UiInput
                v-model="sourcePrefix"
                :label="t('source.prefix')"
                placeholder="photos/"
              />
            </div>
          </div>
        </template>

        <!-- Step 2: Target -->
        <template #target>
          <div class="space-y-4 pt-4">
            <USelectMenu
              v-model="targetType"
              :items="targetProviderTypes"
              value-key="value"
              :label="t('target.type')"
              class="w-full"
            />

            <!-- Local folder picker -->
            <div v-if="targetType === 'local'" class="space-y-2">
              <UButton
                icon="i-lucide-folder"
                color="neutral"
                variant="outline"
                block
                @click="selectTargetFolderAsync"
              >
                {{ targetPath || t('target.selectFolder') }}
              </UButton>
            </div>

            <!-- Peer: space + device + folder -->
            <div v-if="targetType === 'peer'" class="space-y-3">
              <UiSelectMenu
                v-model="targetSpaceId"
                :items="spaceOptions"
                :label="t('target.space')"
                value-key="value"
              />
              <UiSelectMenu
                v-if="targetSpaceId"
                v-model="targetDeviceEndpointId"
                :items="deviceOptionsForSpace(targetSpaceId)"
                :label="t('target.device')"
                value-key="value"
              />
              <template v-if="targetDeviceEndpointId">
                <!-- Toggle: existing folder vs new folder -->
                <div class="flex items-center gap-2">
                  <label class="text-sm font-medium flex-1">{{ t('target.folder') }}</label>
                  <UButton
                    size="xs"
                    variant="link"
                    :icon="targetCreateNewFolder ? 'i-lucide-list' : 'i-lucide-folder-plus'"
                    @click="targetCreateNewFolder = !targetCreateNewFolder; targetShareId = ''; targetNewFolderName = ''"
                  >
                    {{ targetCreateNewFolder ? t('target.chooseExisting') : t('target.createNew') }}
                  </UButton>
                </div>
                <!-- Existing folder -->
                <UiSelectMenu
                  v-if="!targetCreateNewFolder"
                  v-model="targetShareId"
                  :items="shareOptionsForDevice(targetDeviceEndpointId)"
                  value-key="value"
                />
                <!-- New folder name -->
                <UiInput
                  v-else
                  v-model="targetNewFolderName"
                  :placeholder="t('target.newFolderPlaceholder')"
                />
              </template>
              <UiInput
                v-if="targetShareId || targetNewFolderName"
                v-model="targetSubfolder"
                :label="t('target.subfolder')"
                :placeholder="t('target.subfolderPlaceholder')"
              />
            </div>

            <!-- Cloud: backend + prefix -->
            <div v-if="targetType === 'cloud'" class="space-y-3">
              <UiSelectMenu
                v-model="targetBackendId"
                :items="backendOptions"
                :label="t('target.backend')"
                value-key="value"
              />
              <UiInput
                v-model="targetPrefix"
                :label="t('target.prefix')"
                placeholder="backup/"
              />
            </div>
          </div>
        </template>

        <!-- Step 3: Settings -->
        <template #settings>
          <div class="space-y-4 pt-4">
            <!-- Direction -->
            <div>
              <label class="text-sm font-medium">{{ t('settings.direction') }}</label>
              <div class="flex gap-2 mt-1">
                <UiButton
                  :variant="direction === 'one_way' ? 'solid' : 'outline'"
                  icon="i-lucide-arrow-right"
                  @click="direction = 'one_way'"
                >
                  {{ t('settings.oneWay') }}
                </UiButton>
                <UiButton
                  :variant="direction === 'two_way' ? 'solid' : 'outline'"
                  icon="i-lucide-arrow-left-right"
                  @click="direction = 'two_way'"
                >
                  {{ t('settings.twoWay') }}
                </UiButton>
              </div>
            </div>

            <!-- Delete mode -->
            <UiSelectMenu
              v-model="deleteMode"
              :items="deleteModeOptions"
              :label="t('settings.deleteMode')"
              value-key="value"
            />
          </div>
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
            {{ step > 0 ? t('actions.back') : t('actions.cancel') }}
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
          v-if="step < 2"
          icon="i-lucide-arrow-right"
          :disabled="!canProceed"
          @click="step++"
        >
          {{ t('actions.next') }}
        </UiButton>
        <UiButton
          v-else
          icon="i-lucide-check"
          color="primary"
          :loading="isCreating"
          :disabled="!canCreate"
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
import { eq } from 'drizzle-orm'
import { haexDevices, type SelectHaexSyncRules } from '~/database/schemas'
import { getUcanForSpaceAsync } from '~/utils/auth/ucanStore'

type ProviderType = 'local' | 'peer' | 'cloud'

const open = defineModel<boolean>('open', { required: true })

const props = defineProps<{
  editRule?: SelectHaexSyncRules | null
  prefill?: {
    sourceType: 'local' | 'peer'
    spaceId: string
    deviceEndpointId: string
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
const syncBackendsStore = useSyncBackendsStore()
const deviceStore = useDeviceStore()
const { currentVault } = storeToRefs(useVaultStore())

// UStepper uses 0-based index
const step = ref(0)
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

// -- Source state --
const sourceType = ref<ProviderType>('local')
const sourcePath = ref('')
const sourceSpaceId = ref('')
const sourceDeviceEndpointId = ref('')
const sourceShareId = ref('')
const sourceSubfolder = ref('')
const sourceBackendId = ref('')
const sourcePrefix = ref('')

// -- Target state --
const targetType = ref<ProviderType>('local')
const targetPath = ref('')
const targetSpaceId = ref('')
const targetDeviceEndpointId = ref('')
const targetShareId = ref('')
const targetCreateNewFolder = ref(false)
const targetNewFolderName = ref('')
const targetSubfolder = ref('')
const targetBackendId = ref('')
const targetPrefix = ref('')

// -- Settings state --
const direction = ref<'one_way' | 'two_way'>('one_way')
const intervalSeconds = ref(300)
const deleteMode = ref('trash')

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
  syncBackendsStore.backends.map(b => ({ label: b.name, value: b.id })),
)

const deviceOptionsForSpace = (spaceId: string) =>
  peerStorageStore.spaceDevices
    .filter(d => d.spaceId === spaceId)
    .map(d => ({ label: d.deviceName, value: d.deviceEndpointId }))

const shareOptionsForDevice = (endpointId: string) =>
  peerStorageStore.shares
    .filter(s => s.deviceEndpointId === endpointId)
    .map(s => ({ label: s.name, value: s.id }))

// -- Validation --
const isSourceValid = computed(() => {
  switch (sourceType.value) {
    case 'local': return !!sourcePath.value
    case 'peer': return !!sourceShareId.value
    case 'cloud': return !!sourceBackendId.value
    default: return false
  }
})

const isTargetValid = computed(() => {
  switch (targetType.value) {
    case 'local': return !!targetPath.value
    case 'peer': return targetCreateNewFolder.value
      ? !!targetNewFolderName.value.trim()
      : !!targetShareId.value
    case 'cloud': return !!targetBackendId.value
    default: return false
  }
})

const canProceed = computed(() => {
  if (step.value === 0) return isSourceValid.value
  if (step.value === 1) return isTargetValid.value
  return true
})

const canCreate = computed(() =>
  isSourceValid.value && isTargetValid.value,
)



// -- Folder selection --
const selectSourceFolderAsync = async () => {
  const path = await invoke<string | null>('filesystem_select_folder', {})
  if (path) sourcePath.value = path
}

const selectTargetFolderAsync = async () => {
  const path = await invoke<string | null>('filesystem_select_folder', {})
  if (path) targetPath.value = path
}

// -- Build config objects --
const buildSourceConfig = () => {
  switch (sourceType.value) {
    case 'local': return { path: sourcePath.value }
    case 'peer': {
      const spaceId = sourceSpaceId.value
      const ucanToken = spaceId ? getUcanForSpaceAsync(spaceId) : null
      if (!ucanToken) throw new Error('No valid UCAN token for this space')
      const basePath = sourceShareId.value
      const sub = sourceSubfolder.value.trim().replace(/^\/+|\/+$/g, '')
      const path = sub ? `${basePath}/${sub}` : basePath
      return {
        endpointId: sourceDeviceEndpointId.value,
        path,
        spaceId,
        ucanToken,
      }
    }
    case 'cloud': return {
      backendId: sourceBackendId.value,
      prefix: sourcePrefix.value,
    }
  }
}

const buildTargetConfig = () => {
  switch (targetType.value) {
    case 'local': return { path: targetPath.value }
    case 'peer': {
      const spaceId = targetSpaceId.value
      const ucanToken = spaceId ? getUcanForSpaceAsync(spaceId) : null
      if (!ucanToken) throw new Error('No valid UCAN token for this space')
      const basePath = targetCreateNewFolder.value
        ? targetNewFolderName.value.trim()
        : targetShareId.value
      const sub = targetSubfolder.value.trim().replace(/^\/+|\/+$/g, '')
      const path = sub ? `${basePath}/${sub}` : basePath
      return {
        endpointId: targetDeviceEndpointId.value,
        path,
        spaceId,
        ucanToken,
      }
    }
    case 'cloud': return {
      backendId: targetBackendId.value,
      prefix: targetPrefix.value,
    }
  }
}

// -- Resolve current device DB id --
const resolveCurrentDeviceIdAsync = async (): Promise<string> => {
  const db = currentVault.value?.drizzle
  if (!db) throw new Error('No vault open')

  const endpointId = deviceStore.deviceId
  if (!endpointId) throw new Error('Device not initialized')

  const rows = await db
    .select()
    .from(haexDevices)
    .where(eq(haexDevices.endpointId, endpointId))

  if (rows.length > 0) return rows[0]!.id

  const id = crypto.randomUUID()
  await db.insert(haexDevices).values({
    id,
    endpointId,
    name: deviceStore.deviceName || deviceStore.hostname || endpointId.slice(0, 12),
    platform: 'desktop',
  })
  return id
}

// -- Determine spaceId for the rule --
const resolveSpaceId = (): string => {
  if (sourceType.value === 'peer' && sourceSpaceId.value) return sourceSpaceId.value
  if (targetType.value === 'peer' && targetSpaceId.value) return targetSpaceId.value
  return spacesStore.spaces[0]?.id ?? ''
}

// -- Navigation --
const onBack = () => {
  if (step.value > 0) {
    step.value--
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
  if (!canCreate.value) return
  isCreating.value = true

  try {
    if (isEditMode.value && props.editRule) {
      await fileSyncStore.updateRuleAsync(props.editRule.id, {
        sourceType: sourceType.value,
        sourceConfig: buildSourceConfig(),
        targetType: targetType.value,
        targetConfig: buildTargetConfig(),
        direction: direction.value,
        syncIntervalSeconds: intervalSeconds.value,
        deleteMode: deleteMode.value,
      })
      addToast({ title: t('success.updated'), color: 'success' })
      emit('updated')
    } else {
      const deviceId = await resolveCurrentDeviceIdAsync()
      const spaceId = resolveSpaceId()
      if (!spaceId) throw new Error('No space available')

      await fileSyncStore.createRuleAsync({
        id: crypto.randomUUID(),
        spaceId,
        deviceId,
        sourceType: sourceType.value,
        sourceConfig: buildSourceConfig(),
        targetType: targetType.value,
        targetConfig: buildTargetConfig(),
        direction: direction.value,
        syncIntervalSeconds: intervalSeconds.value,
        deleteMode: deleteMode.value,
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

// -- Reset on open --
const resetForm = () => {
  step.value = 0
  sourceType.value = 'local'
  sourcePath.value = ''
  sourceSpaceId.value = ''
  sourceDeviceEndpointId.value = ''
  sourceShareId.value = ''
  sourceSubfolder.value = ''
  sourceBackendId.value = ''
  sourcePrefix.value = ''
  targetType.value = 'local'
  targetPath.value = ''
  targetSpaceId.value = ''
  targetDeviceEndpointId.value = ''
  targetShareId.value = ''
  targetCreateNewFolder.value = false
  targetNewFolderName.value = ''
  targetSubfolder.value = ''
  targetBackendId.value = ''
  targetPrefix.value = ''
  direction.value = 'one_way'
  intervalSeconds.value = 300
  deleteMode.value = 'trash'
}

const populateFromRule = (rule: SelectHaexSyncRules) => {
  const srcCfg = rule.sourceConfig as Record<string, unknown>
  const tgtCfg = rule.targetConfig as Record<string, unknown>

  sourceType.value = rule.sourceType as ProviderType
  targetType.value = rule.targetType as ProviderType
  direction.value = rule.direction as 'one_way' | 'two_way'
  intervalSeconds.value = rule.syncIntervalSeconds
  deleteMode.value = rule.deleteMode

  // Source
  if (rule.sourceType === 'local') {
    sourcePath.value = (srcCfg?.path as string) || ''
  } else if (rule.sourceType === 'peer') {
    sourceSpaceId.value = (srcCfg?.spaceId as string) || ''
    sourceDeviceEndpointId.value = (srcCfg?.endpointId as string) || ''
    sourceShareId.value = (srcCfg?.path as string) || ''
  } else if (rule.sourceType === 'cloud') {
    sourceBackendId.value = (srcCfg?.backendId as string) || ''
    sourcePrefix.value = (srcCfg?.prefix as string) || ''
  }

  // Target
  if (rule.targetType === 'local') {
    targetPath.value = (tgtCfg?.path as string) || ''
  } else if (rule.targetType === 'peer') {
    targetSpaceId.value = (tgtCfg?.spaceId as string) || ''
    targetDeviceEndpointId.value = (tgtCfg?.endpointId as string) || ''
    targetShareId.value = (tgtCfg?.path as string) || ''
  } else if (rule.targetType === 'cloud') {
    targetBackendId.value = (tgtCfg?.backendId as string) || ''
    targetPrefix.value = (tgtCfg?.prefix as string) || ''
  }
}

watch(open, async (isOpen) => {
  if (isOpen) {
    resetForm()
    await peerStorageStore.loadSharesAsync()
    await peerStorageStore.loadSpaceDevicesAsync()
    if (props.editRule) {
      populateFromRule(props.editRule)
    } else if (props.prefill) {
      sourceType.value = props.prefill.sourceType
      if (props.prefill.sourceType === 'local' && props.prefill.localPath) {
        sourcePath.value = props.prefill.localPath
      } else {
        sourceSpaceId.value = props.prefill.spaceId
        sourceDeviceEndpointId.value = props.prefill.deviceEndpointId
        sourceShareId.value = props.prefill.shareName
      }
      step.value = 1 // Jump to target step
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
</i18n>
