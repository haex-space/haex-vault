import { invoke } from '@tauri-apps/api/core'
import type { ExtensionPermission } from '~~/src-tauri/bindings/ExtensionPermission'
import type { PermissionEntry } from '~~/src-tauri/bindings/PermissionEntry'

export interface ExtensionPermissionsEditable {
  database?: PermissionEntry[] | null
  filesystem?: PermissionEntry[] | null
  http?: PermissionEntry[] | null
  shell?: PermissionEntry[] | null
  syncServers?: PermissionEntry[] | null
  cloudStorage?: PermissionEntry[] | null
  syncRules?: PermissionEntry[] | null
}

const emptyPermissions = (): ExtensionPermissionsEditable => ({
  database: null,
  filesystem: null,
  http: null,
  shell: null,
  syncServers: null,
  cloudStorage: null,
  syncRules: null,
})

export const useExtensionDetailState = () => {
  const { t } = useI18n()
  const { add } = useToast()

  // Persistent permissions
  const loadingPermissions = ref(true)
  const savingPermissions = ref(false)
  const originalPermissions = ref<ExtensionPermissionsEditable>(emptyPermissions())
  const editablePermissions = ref<ExtensionPermissionsEditable>(emptyPermissions())

  // Session permissions (in-memory, not persisted)
  const sessionPermissions = ref<ExtensionPermission[]>([])
  const revokingSessionPermission = ref<string | null>(null)

  const hasAnyPermissions = computed(() => {
    return (
      (editablePermissions.value.database?.length ?? 0) > 0 ||
      (editablePermissions.value.filesystem?.length ?? 0) > 0 ||
      (editablePermissions.value.http?.length ?? 0) > 0 ||
      (editablePermissions.value.shell?.length ?? 0) > 0 ||
      (editablePermissions.value.syncServers?.length ?? 0) > 0 ||
      (editablePermissions.value.cloudStorage?.length ?? 0) > 0 ||
      (editablePermissions.value.syncRules?.length ?? 0) > 0
    )
  })

  const hasPermissionChanges = computed(() => {
    const compareArrays = (
      a: PermissionEntry[] | null | undefined,
      b: PermissionEntry[] | null | undefined,
    ) => {
      if (!a && !b) return true
      if (!a || !b) return false
      if (a.length !== b.length) return false
      return a.every((item, index) => {
        const other = b[index]
        return item.target === other?.target && item.status === other?.status
      })
    }

    return (
      !compareArrays(editablePermissions.value.database, originalPermissions.value.database) ||
      !compareArrays(editablePermissions.value.filesystem, originalPermissions.value.filesystem) ||
      !compareArrays(editablePermissions.value.http, originalPermissions.value.http) ||
      !compareArrays(editablePermissions.value.shell, originalPermissions.value.shell) ||
      !compareArrays(editablePermissions.value.syncServers, originalPermissions.value.syncServers) ||
      !compareArrays(editablePermissions.value.cloudStorage, originalPermissions.value.cloudStorage) ||
      !compareArrays(editablePermissions.value.syncRules, originalPermissions.value.syncRules)
    )
  })

  const permissionAccordionItems = computed(() => {
    const items = []

    if ((editablePermissions.value.database?.length ?? 0) > 0) {
      items.push({
        label: t('permissionTypes.database'),
        icon: 'i-heroicons-circle-stack',
        slot: 'database',
        defaultOpen: true,
      })
    }

    if ((editablePermissions.value.filesystem?.length ?? 0) > 0) {
      items.push({
        label: t('permissionTypes.filesystem'),
        icon: 'i-heroicons-folder',
        slot: 'filesystem',
      })
    }

    if ((editablePermissions.value.http?.length ?? 0) > 0) {
      items.push({
        label: t('permissionTypes.http'),
        icon: 'i-heroicons-globe-alt',
        slot: 'http',
      })
    }

    if ((editablePermissions.value.shell?.length ?? 0) > 0) {
      items.push({
        label: t('permissionTypes.shell'),
        icon: 'i-heroicons-command-line',
        slot: 'shell',
      })
    }

    if ((editablePermissions.value.syncServers?.length ?? 0) > 0) {
      items.push({
        label: t('permissionTypes.syncServers'),
        icon: 'i-heroicons-server',
        slot: 'syncServers',
      })
    }

    if ((editablePermissions.value.cloudStorage?.length ?? 0) > 0) {
      items.push({
        label: t('permissionTypes.cloudStorage'),
        icon: 'i-heroicons-cloud-arrow-up',
        slot: 'cloudStorage',
      })
    }

    if ((editablePermissions.value.syncRules?.length ?? 0) > 0) {
      items.push({
        label: t('permissionTypes.syncRules'),
        icon: 'i-heroicons-arrow-path',
        slot: 'syncRules',
      })
    }

    return items
  })

  const loadPermissionsAsync = async (extensionId: string) => {
    loadingPermissions.value = true
    try {
      const permissions = await invoke<ExtensionPermissionsEditable>(
        'get_extension_permissions',
        {
          extensionId,
        },
      )
      // Store original for comparison
      originalPermissions.value = JSON.parse(JSON.stringify(permissions))
      editablePermissions.value = permissions
    } catch (error) {
      console.error('Error loading permissions:', error)
      editablePermissions.value = emptyPermissions()
      originalPermissions.value = emptyPermissions()
      add({ description: t('permissionsLoadError'), color: 'error' })
    } finally {
      loadingPermissions.value = false
    }
  }

  const savePermissionsAsync = async (extensionId: string) => {
    savingPermissions.value = true
    try {
      await invoke('update_extension_permissions', {
        extensionId,
        permissions: editablePermissions.value,
      })
      // Update original after successful save
      originalPermissions.value = JSON.parse(JSON.stringify(editablePermissions.value))
      add({ description: t('permissionsSaved'), color: 'success' })
    } catch (error) {
      console.error('Error saving permissions:', error)
      add({ description: t('permissionsSaveError'), color: 'error' })
    } finally {
      savingPermissions.value = false
    }
  }

  const loadSessionPermissionsAsync = async (extensionId: string) => {
    try {
      sessionPermissions.value = await invoke<ExtensionPermission[]>(
        'get_extension_session_permissions',
        { extensionId },
      )
    } catch (error) {
      console.error('Error loading session permissions:', error)
      sessionPermissions.value = []
    }
  }

  const revokeSessionPermissionAsync = async (
    extensionId: string,
    permission: ExtensionPermission,
  ) => {
    const key = `${permission.resourceType}-${permission.target}`
    revokingSessionPermission.value = key
    try {
      await invoke('remove_extension_session_permission', {
        extensionId,
        resourceType: permission.resourceType,
        target: permission.target,
      })
      add({ description: t('sessionPermissionRevoked'), color: 'success' })
      await loadSessionPermissionsAsync(extensionId)
    } catch (error) {
      console.error('Error revoking session permission:', error)
      add({ description: t('sessionPermissionRevokeError'), color: 'error' })
    } finally {
      revokingSessionPermission.value = null
    }
  }

  return {
    loadingPermissions,
    savingPermissions,
    originalPermissions,
    editablePermissions,
    sessionPermissions,
    revokingSessionPermission,
    hasAnyPermissions,
    hasPermissionChanges,
    permissionAccordionItems,
    loadPermissionsAsync,
    savePermissionsAsync,
    loadSessionPermissionsAsync,
    revokeSessionPermissionAsync,
  }
}
