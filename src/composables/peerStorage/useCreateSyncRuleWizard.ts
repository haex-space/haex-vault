import type { ComputedRef } from 'vue'

export type ProviderType = 'local' | 'peer' | 'cloud'

export type SourceState = {
  type: Ref<ProviderType>
  path: Ref<string>
  spaceId: Ref<string>
  deviceEndpointId: Ref<string>
  shareId: Ref<string>
  subfolder: Ref<string>
  backendId: Ref<string>
  bucket: Ref<string>
  prefix: Ref<string>
}

export type TargetState = {
  type: Ref<ProviderType>
  path: Ref<string>
  spaceId: Ref<string>
  deviceEndpointId: Ref<string>
  shareId: Ref<string>
  createNewFolder: Ref<boolean>
  newFolderName: Ref<string>
  subfolder: Ref<string>
  backendId: Ref<string>
  bucket: Ref<string>
  prefix: Ref<string>
}

export type SettingsState = {
  direction: Ref<'one_way' | 'two_way'>
  intervalSeconds: Ref<number>
  deleteMode: Ref<string>
}

/**
 * Wizard state + validation for the CreateSyncRuleDialog.
 * Owns step index and per-step refs. Parent dialog wires these into
 * build/save logic and step children.
 */
export const useCreateSyncRuleWizard = () => {
  // UStepper uses 0-based index
  const step = ref(0)

  const source: SourceState = {
    type: ref<ProviderType>('local'),
    path: ref(''),
    spaceId: ref(''),
    deviceEndpointId: ref(''),
    shareId: ref(''),
    subfolder: ref(''),
    backendId: ref(''),
    bucket: ref(''),
    prefix: ref(''),
  }

  const target: TargetState = {
    type: ref<ProviderType>('local'),
    path: ref(''),
    spaceId: ref(''),
    deviceEndpointId: ref(''),
    shareId: ref(''),
    createNewFolder: ref(false),
    newFolderName: ref(''),
    subfolder: ref(''),
    backendId: ref(''),
    bucket: ref(''),
    prefix: ref(''),
  }

  const settings: SettingsState = {
    direction: ref<'one_way' | 'two_way'>('one_way'),
    intervalSeconds: ref(300),
    deleteMode: ref('trash'),
  }

  const isSourceValid: ComputedRef<boolean> = computed(() => {
    switch (source.type.value) {
      case 'local': return !!source.path.value
      case 'peer':
        return !!source.spaceId.value
          && !!source.deviceEndpointId.value
          && !!source.shareId.value
      case 'cloud': return !!source.backendId.value
      default: return false
    }
  })

  const isTargetValid: ComputedRef<boolean> = computed(() => {
    switch (target.type.value) {
      case 'local': return !!target.path.value
      case 'peer': {
        const hasPeerBase = !!target.spaceId.value && !!target.deviceEndpointId.value
        const hasFolder = target.createNewFolder.value
          ? !!target.newFolderName.value.trim()
          : !!target.shareId.value
        return hasPeerBase && hasFolder
      }
      case 'cloud': return !!target.backendId.value
      default: return false
    }
  })

  const canProceed = computed(() => {
    if (step.value === 0) return isSourceValid.value
    if (step.value === 1) return isTargetValid.value
    return true
  })

  const canCreate = computed(() => isSourceValid.value && isTargetValid.value)

  const reset = () => {
    step.value = 0
    source.type.value = 'local'
    source.path.value = ''
    source.spaceId.value = ''
    source.deviceEndpointId.value = ''
    source.shareId.value = ''
    source.subfolder.value = ''
    source.backendId.value = ''
    source.bucket.value = ''
    source.prefix.value = ''
    target.type.value = 'local'
    target.path.value = ''
    target.spaceId.value = ''
    target.deviceEndpointId.value = ''
    target.shareId.value = ''
    target.createNewFolder.value = false
    target.newFolderName.value = ''
    target.subfolder.value = ''
    target.backendId.value = ''
    target.bucket.value = ''
    target.prefix.value = ''
    settings.direction.value = 'one_way'
    settings.intervalSeconds.value = 300
    settings.deleteMode.value = 'trash'
  }

  return {
    step,
    source,
    target,
    settings,
    isSourceValid,
    isTargetValid,
    canProceed,
    canCreate,
    reset,
  }
}
