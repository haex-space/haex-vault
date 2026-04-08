<template>
  <UiDrawerModal
    v-model:open="open"
    :title="t('title')"
    :description="t('description')"
  >
    <template #body>
      <!-- Step indicator -->
      <div class="flex gap-2 mb-4">
        <UBadge
          v-for="s in steps"
          :key="s.number"
          :variant="step >= s.number ? 'solid' : 'outline'"
          :color="step >= s.number ? 'primary' : 'neutral'"
          size="sm"
          class="cursor-pointer"
          @click="s.number < step ? step = s.number : undefined"
        >
          {{ s.number }}. {{ s.label }}
        </UBadge>
      </div>

      <!-- Step 1: Source -->
      <div v-if="step === 1" class="space-y-4">
        <h3 class="text-sm font-medium">{{ t('source.title') }}</h3>

        <!-- Provider type selector -->
        <div class="flex gap-2">
          <UiButton
            v-for="providerType in providerTypes"
            :key="providerType.value"
            :variant="sourceType === providerType.value ? 'solid' : 'outline'"
            :icon="providerType.icon"
            size="sm"
            @click="sourceType = providerType.value"
          >
            {{ providerType.label }}
          </UiButton>
        </div>

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

      <!-- Step 2: Target -->
      <div v-if="step === 2" class="space-y-4">
        <h3 class="text-sm font-medium">{{ t('target.title') }}</h3>

        <!-- Provider type selector -->
        <div class="flex gap-2">
          <UiButton
            v-for="providerType in providerTypes"
            :key="providerType.value"
            :variant="targetType === providerType.value ? 'solid' : 'outline'"
            :icon="providerType.icon"
            size="sm"
            @click="targetType = providerType.value"
          >
            {{ providerType.label }}
          </UiButton>
        </div>

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

        <!-- Peer: space + device + share pickers -->
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
          <UiSelectMenu
            v-if="targetDeviceEndpointId"
            v-model="targetShareId"
            :items="shareOptionsForDevice(targetDeviceEndpointId)"
            :label="t('target.share')"
            value-key="value"
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

      <!-- Step 3: Settings -->
      <div v-if="step === 3" class="space-y-4">
        <h3 class="text-sm font-medium">{{ t('settings.title') }}</h3>

        <!-- Direction -->
        <div>
          <label class="text-sm font-medium">{{ t('settings.direction') }}</label>
          <div class="flex gap-2 mt-1">
            <UiButton
              :variant="direction === 'one_way' ? 'solid' : 'outline'"
              icon="i-lucide-arrow-right"
              size="sm"
              @click="direction = 'one_way'"
            >
              {{ t('settings.oneWay') }}
            </UiButton>
            <UiButton
              :variant="direction === 'two_way' ? 'solid' : 'outline'"
              icon="i-lucide-arrow-left-right"
              size="sm"
              @click="direction = 'two_way'"
            >
              {{ t('settings.twoWay') }}
            </UiButton>
          </div>
        </div>

        <!-- Sync interval -->
        <UFormField :label="t('settings.interval')">
          <USelectMenu
            v-model="intervalSeconds"
            :items="intervalOptions"
            value-key="value"
            class="w-full"
          />
        </UFormField>

        <!-- Delete mode -->
        <UFormField :label="t('settings.deleteMode')">
          <USelectMenu
            v-model="deleteMode"
            :items="deleteModeOptions"
            value-key="value"
            class="w-full"
          />
        </UFormField>

        <!-- Rule name -->
        <UiInput
          v-model="ruleName"
          :label="t('settings.name')"
          :placeholder="t('settings.namePlaceholder')"
        />
      </div>
    </template>

    <template #footer>
      <div class="flex justify-between gap-4">
        <UiButton
          color="neutral"
          variant="outline"
          @click="onBack"
        >
          {{ step > 1 ? t('actions.back') : t('actions.cancel') }}
        </UiButton>

        <UiButton
          v-if="step < 3"
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
          @click="onCreateAsync"
        >
          {{ t('actions.create') }}
        </UiButton>
      </div>
    </template>
  </UiDrawerModal>
</template>

<script setup lang="ts">
import { invoke } from '@tauri-apps/api/core'
import { eq } from 'drizzle-orm'
import { haexDevices } from '~/database/schemas'

type ProviderType = 'local' | 'peer' | 'cloud'

const open = defineModel<boolean>('open', { required: true })

const emit = defineEmits<{
  created: []
}>()

const { t } = useI18n()
const { add: addToast } = useToast()
const fileSyncStore = useFileSyncStore()
const spacesStore = useSpacesStore()
const peerStorageStore = usePeerStorageStore()
const syncBackendsStore = useSyncBackendsStore()
const deviceStore = useDeviceStore()
const { currentVault } = storeToRefs(useVaultStore())

const step = ref(1)
const isCreating = ref(false)

// -- Provider types --
const providerTypes = computed(() => [
  { value: 'local' as ProviderType, label: t('provider.local'), icon: 'i-lucide-folder' },
  { value: 'peer' as ProviderType, label: t('provider.peer'), icon: 'i-lucide-monitor-smartphone' },
  { value: 'cloud' as ProviderType, label: t('provider.cloud'), icon: 'i-lucide-cloud' },
])

const steps = computed(() => [
  { number: 1, label: t('steps.source') },
  { number: 2, label: t('steps.target') },
  { number: 3, label: t('steps.settings') },
])

// -- Source state --
const sourceType = ref<ProviderType>('local')
const sourcePath = ref('')
const sourceSpaceId = ref('')
const sourceDeviceEndpointId = ref('')
const sourceShareId = ref('')
const sourceBackendId = ref('')
const sourcePrefix = ref('')

// -- Target state --
const targetType = ref<ProviderType>('local')
const targetPath = ref('')
const targetSpaceId = ref('')
const targetDeviceEndpointId = ref('')
const targetShareId = ref('')
const targetBackendId = ref('')
const targetPrefix = ref('')

// -- Settings state --
const direction = ref<'one_way' | 'two_way'>('one_way')
const intervalSeconds = ref<{ label: string; value: number }>()
const deleteMode = ref<{ label: string; value: string }>()
const ruleName = ref('')

// -- Options --
const intervalOptions = computed(() => [
  { label: t('intervals.1min'), value: 60 },
  { label: t('intervals.5min'), value: 300 },
  { label: t('intervals.15min'), value: 900 },
  { label: t('intervals.30min'), value: 1800 },
  { label: t('intervals.1hour'), value: 3600 },
  { label: t('intervals.manual'), value: 0 },
])

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
    case 'peer': return !!targetShareId.value
    case 'cloud': return !!targetBackendId.value
    default: return false
  }
})

const canProceed = computed(() => {
  if (step.value === 1) return isSourceValid.value
  if (step.value === 2) return isTargetValid.value
  return true
})

const canCreate = computed(() =>
  isSourceValid.value && isTargetValid.value && !!ruleName.value.trim(),
)

// -- Auto-generate rule name --
const autoGenerateName = () => {
  if (ruleName.value) return // Don't overwrite manual edits

  let name = ''
  if (sourceType.value === 'local' && sourcePath.value) {
    name = sourcePath.value.split(/[/\\]/).pop() || sourcePath.value
  } else if (sourceType.value === 'peer' && sourceShareId.value) {
    const share = peerStorageStore.shares.find(s => s.id === sourceShareId.value)
    name = share?.name || ''
  } else if (sourceType.value === 'cloud' && sourcePrefix.value) {
    name = sourcePrefix.value.replace(/\/$/, '').split('/').pop() || 'cloud-sync'
  }

  if (name) {
    ruleName.value = name
  }
}

// Watch source changes to auto-generate name
watch([sourcePath, sourceShareId, sourcePrefix], autoGenerateName)

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
    case 'peer': return {
      endpointId: sourceDeviceEndpointId.value,
      shareId: sourceShareId.value,
      spaceId: sourceSpaceId.value,
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
    case 'peer': return {
      endpointId: targetDeviceEndpointId.value,
      shareId: targetShareId.value,
      spaceId: targetSpaceId.value,
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

  // Device record doesn't exist yet -- create one
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
  // Use whichever side has a peer provider -- that's the space context
  if (sourceType.value === 'peer' && sourceSpaceId.value) return sourceSpaceId.value
  if (targetType.value === 'peer' && targetSpaceId.value) return targetSpaceId.value
  // For local-to-local or cloud rules, pick first active space or vault space
  return spacesStore.spaces[0]?.id ?? ''
}

// -- Navigation --
const onBack = () => {
  if (step.value > 1) {
    step.value--
  } else {
    open.value = false
  }
}

// -- Create rule --
const onCreateAsync = async () => {
  if (!canCreate.value) return
  isCreating.value = true

  try {
    const deviceId = await resolveCurrentDeviceIdAsync()
    const spaceId = resolveSpaceId()
    if (!spaceId) throw new Error('No space available')

    await fileSyncStore.createRuleAsync({
      id: crypto.randomUUID(),
      spaceId,
      deviceId,
      name: ruleName.value.trim(),
      sourceType: sourceType.value,
      sourceConfig: buildSourceConfig(),
      targetType: targetType.value,
      targetConfig: buildTargetConfig(),
      direction: direction.value,
      syncIntervalSeconds: intervalSeconds.value?.value ?? 300,
      deleteMode: deleteMode.value?.value ?? 'trash',
      enabled: true,
    })

    addToast({ title: t('success.created'), color: 'success' })
    open.value = false
    emit('created')
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
  step.value = 1
  sourceType.value = 'local'
  sourcePath.value = ''
  sourceSpaceId.value = ''
  sourceDeviceEndpointId.value = ''
  sourceShareId.value = ''
  sourceBackendId.value = ''
  sourcePrefix.value = ''
  targetType.value = 'local'
  targetPath.value = ''
  targetSpaceId.value = ''
  targetDeviceEndpointId.value = ''
  targetShareId.value = ''
  targetBackendId.value = ''
  targetPrefix.value = ''
  direction.value = 'one_way'
  intervalSeconds.value = undefined
  deleteMode.value = undefined
  ruleName.value = ''
}

watch(open, async (isOpen) => {
  if (isOpen) {
    resetForm()
    intervalSeconds.value = intervalOptions.value[1] // 5 min default
    deleteMode.value = deleteModeOptions.value[0] // trash default
    await peerStorageStore.loadSharesAsync()
    await peerStorageStore.loadSpaceDevicesAsync()
  }
})
</script>

<i18n lang="yaml">
de:
  title: Sync-Regel erstellen
  description: Dateien automatisch zwischen Quell- und Zielordner synchronisieren
  steps:
    source: Quelle
    target: Ziel
    settings: Einstellungen
  provider:
    local: Lokaler Ordner
    peer: P2P Peer
    cloud: Cloud-Speicher
  source:
    title: Quelle auswählen
    selectFolder: Ordner auswählen
    space: Space
    device: Gerät
    share: Freigabe
    backend: Storage-Backend
    prefix: Pfad-Präfix
  target:
    title: Ziel auswählen
    selectFolder: Ordner auswählen
    space: Space
    device: Gerät
    share: Freigabe
    backend: Storage-Backend
    prefix: Pfad-Präfix
  settings:
    title: Synchronisationseinstellungen
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
  success:
    created: Sync-Regel erstellt
  errors:
    createFailed: Sync-Regel konnte nicht erstellt werden
en:
  title: Create Sync Rule
  description: Automatically synchronize files between source and target
  steps:
    source: Source
    target: Target
    settings: Settings
  provider:
    local: Local Folder
    peer: P2P Peer
    cloud: Cloud Storage
  source:
    title: Choose source
    selectFolder: Select folder
    space: Space
    device: Device
    share: Share
    backend: Storage backend
    prefix: Path prefix
  target:
    title: Choose target
    selectFolder: Select folder
    space: Space
    device: Device
    share: Share
    backend: Storage backend
    prefix: Path prefix
  settings:
    title: Sync settings
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
  success:
    created: Sync rule created
  errors:
    createFailed: Failed to create sync rule
</i18n>
