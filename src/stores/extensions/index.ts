import { invoke } from '@tauri-apps/api/core'
import { readFile } from '@tauri-apps/plugin-fs'
import { getExtensionUrl } from '~/utils/extension'

import type {
  IHaexSpaceExtension,
  IHaexSpaceExtensionManifest,
} from '~/types/haexspace'
import type { ExtensionPreview } from '@bindings/ExtensionPreview'
import type { ExtensionPermissions } from '~~/src-tauri/bindings/ExtensionPermissions'
import type { ExtensionInfoResponse } from '~~/src-tauri/bindings/ExtensionInfoResponse'
import type { DisplayMode } from '~~/src-tauri/bindings/DisplayMode'

/* const manifestFileName = 'manifest.json'
const logoFileName = 'icon.svg' */

export const useExtensionsStore = defineStore('extensionsStore', () => {
  const availableExtensions = ref<IHaexSpaceExtension[]>([])
  const currentRoute = useRouter().currentRoute

  const currentExtensionId = computed(() =>
    getSingleRouteParam(currentRoute.value.params.extensionId),
  )

  const currentExtension = computed(() => {
    if (!currentExtensionId.value) return null

    return (
      availableExtensions.value.find(
        (ext) => ext.id === currentExtensionId.value,
      ) ?? null
    )
  })

  /* const isActive = (id: string) =>
    computed(
      () =>
        currentRoute.value.name === 'extension' &&
        currentRoute.value.params.extensionId === id,
    ) */

  const extensionEntry = computed(() => {
    if (
      !currentExtension.value?.version ||
      !currentExtension.value?.publicKey ||
      !currentExtension.value?.name
    )
      return null

    return getExtensionUrl(
      currentExtension.value.publicKey,
      currentExtension.value.name,
      currentExtension.value.version,
      currentExtension.value.entry ?? 'index.html',
      currentExtension.value.devServerUrl ?? undefined,
    )
  })

  /* const getExtensionPathAsync = async (
    extensionId?: string,
    version?: string,
  ) => {
    if (!extensionId || !version) return ''
    return await join(await appDataDir(), 'extensions', extensionId, version)
  } */

  /* const checkSourceExtensionDirectoryAsync = async (
    extensionDirectory: string,
  ) => {
    try {
      const dir = await readDir(extensionDirectory)
      const manifest = dir.find(
        (entry) => entry.name === manifestFileName && entry.isFile,
      )
      if (!manifest) throw new Error('Kein Manifest für Erweiterung gefunden')

      const logo = dir.find((item) => item.isFile && item.name === logoFileName)
      if (!logo) throw new Error('Logo fehlt')
      console.log('found icon', logo)

      return true
    } catch (error) {
      console.error(error)
      addNotificationAsync({ type: 'error', text: JSON.stringify(error) })
      //throw error //new Error(`Keine Leseberechtigung für Ordner ${extensionDirectory}`);
    }
  } */

  const loadExtensionsAsync = async () => {
    try {
      const extensions =
        await invoke<ExtensionInfoResponse[]>('get_all_extensions')

      console.log('get_all_extensions', extensions)
      // ExtensionInfoResponse is now directly compatible with IHaexSpaceExtension
      availableExtensions.value = extensions
    } catch (error) {
      console.error('Fehler beim Laden der Extensions:', error)
      throw error
    }
  }

  /* const loadExtensionsAsync = async () => {
    const { currentVault } = storeToRefs(useVaultStore())

    const extensions =
      (await currentVault.value?.drizzle.select().from(haexExtensions)) ?? []

    //if (!extensions?.length) return false;

    const installedExtensions = await filterAsync(
      extensions,
      isExtensionInstalledAsync,
    )
    console.log('loadExtensionsAsync installedExtensions', installedExtensions)

    availableExtensions.value =
      extensions.map((extension) => ({
        id: extension.id,
        name: extension.name ?? '',
        icon: extension.icon ?? '',
        author: extension.author ?? '',
        version: extension.version ?? '',
        enabled: extension.enabled ? true : false,
        installed: installedExtensions.includes(extension),
      })) ?? []

    console.log('loadExtensionsAsync', availableExtensions.value)
    return true
  } */

  // Cached bytes from marketplace download for install after preview
  const pendingInstallBytes = ref<Uint8Array | null>(null)

  const installAsync = async (
    sourcePath: string | null,
    permissions?: ExtensionPermissions,
  ) => {
    if (!sourcePath) throw new Error('Kein Pfad angegeben')

    try {
      // Read file as bytes (works with content URIs on Android)
      const fileBytes = await readFile(sourcePath)

      const extensionId = await invoke<string>(
        'install_extension_with_permissions',
        {
          fileBytes: Array.from(fileBytes),
          customPermissions: permissions,
        },
      )
      return extensionId
    } catch (error) {
      console.error('Fehler bei Extension-Installation:', error)
      throw error
    }
  }

  /**
   * Download extension from URL and show preview
   * Caches bytes for subsequent install
   */
  const downloadAndPreviewAsync = async (downloadUrl: string, expectedHash?: string) => {
    try {
      const response = await fetch(downloadUrl)

      if (!response.ok) {
        throw new Error(`Download fehlgeschlagen: ${response.status} ${response.statusText}`)
      }

      const arrayBuffer = await response.arrayBuffer()
      const fileBytes = new Uint8Array(arrayBuffer)

      // TODO: Verify hash if provided
      if (expectedHash) {
        console.log('Expected hash:', expectedHash)
      }

      // Cache bytes for install
      pendingInstallBytes.value = fileBytes

      // Get preview
      preview.value = await invoke<ExtensionPreview>('preview_extension', {
        fileBytes: Array.from(fileBytes),
      })

      return preview.value
    } catch (error) {
      console.error('Fehler beim Download der Extension:', error)
      pendingInstallBytes.value = null
      throw error
    }
  }

  /**
   * Install the previously downloaded extension (full installation: DB + files)
   */
  const installPendingAsync = async (permissions?: ExtensionPermissions) => {
    if (!pendingInstallBytes.value) {
      throw new Error('Keine Extension zum Installieren vorhanden')
    }

    try {
      const extensionId = await invoke<string>(
        'install_extension_with_permissions',
        {
          fileBytes: Array.from(pendingInstallBytes.value),
          customPermissions: permissions,
        },
      )

      // Clear cache after successful install
      pendingInstallBytes.value = null

      return extensionId
    } catch (error) {
      console.error('Fehler bei Extension-Installation:', error)
      throw error
    }
  }

  /**
   * Register extension metadata in database only.
   * Use this for the first step of a two-step installation.
   * Returns the extension ID.
   */
  const registerInDatabaseAsync = async (
    manifest: IHaexSpaceExtensionManifest,
    permissions?: ExtensionPermissions,
  ) => {
    try {
      const extensionId = await invoke<string>(
        'register_extension_in_database',
        {
          manifest,
          customPermissions: permissions ?? { database: [], filesystem: [], http: [], shell: [] },
        },
      )
      return extensionId
    } catch (error) {
      console.error('Fehler bei DB-Registrierung:', error)
      throw error
    }
  }

  /**
   * Install extension files only (no DB registration).
   * Use when extension already exists in DB (e.g., from sync).
   */
  const installFilesAsync = async (extensionId: string) => {
    if (!pendingInstallBytes.value) {
      throw new Error('Keine Extension zum Installieren vorhanden')
    }

    try {
      const resultId = await invoke<string>(
        'install_extension_files',
        {
          fileBytes: Array.from(pendingInstallBytes.value),
          extensionId,
        },
      )

      // Clear cache after successful install
      pendingInstallBytes.value = null

      return resultId
    } catch (error) {
      console.error('Fehler bei Datei-Installation:', error)
      throw error
    }
  }

  /**
   * Full installation: Register in DB, then install files.
   * Explicitly performs both steps separately.
   */
  const registerAndInstallFilesAsync = async (permissions?: ExtensionPermissions) => {
    if (!pendingInstallBytes.value || !preview.value?.manifest) {
      throw new Error('Keine Extension zum Installieren vorhanden')
    }

    // Step 1: Register in database
    const extensionId = await registerInDatabaseAsync(preview.value.manifest, permissions)

    // Step 2: Install files
    await installFilesAsync(extensionId)

    return extensionId
  }

  const clearPendingInstall = () => {
    pendingInstallBytes.value = null
    preview.value = undefined
  }

  /**
   * Remove an extension
   * @param publicKey - Extension's public key
   * @param name - Extension name
   * @param version - Extension version
   * @param deleteData - If true, also deletes all extension data (tables). If false (default), only removes the extension from this device.
   */
  const removeExtensionAsync = async (
    publicKey: string,
    name: string,
    version: string,
    deleteData: boolean = false,
  ) => {
    try {
      await invoke('remove_extension', {
        publicKey,
        name,
        version,
        deleteData,
      })
    } catch (error) {
      console.error('Fehler beim Entfernen der Extension:', error)
      throw error
    }
  }

  /**
   * Remove a dev extension
   * @param publicKey - Extension's public key
   * @param name - Extension name
   */
  const removeDevExtensionAsync = async (publicKey: string, name: string) => {
    try {
      await invoke('remove_dev_extension', {
        publicKey,
        name,
      })
    } catch (error) {
      console.error('Fehler beim Entfernen der Dev-Extension:', error)
      throw error
    }
  }

  /* const removeExtensionAsync = async (id: string, version: string) => {
    try {
      console.log('remove extension', id, version)
      await removeExtensionFromVaultAsync(id, version)
      await removeExtensionFilesAsync(id, version)
    } catch (error) {
      throw new Error(JSON.stringify(error))
    }
  } */

  const isExtensionInstalledAsync = async ({
    publicKey,
    name,
    version,
  }: {
    publicKey: string
    name: string
    version: string
  }) => {
    try {
      return await invoke<boolean>('is_extension_installed', {
        publicKey,
        name,
        extensionVersion: version,
      })
    } catch (error) {
      console.error('Fehler beim Prüfen der Extension:', error)
      return false
    }
  }

  const checkManifest = (
    manifestFile: unknown,
  ): manifestFile is IHaexSpaceExtensionManifest => {
    const errors = []

    if (typeof manifestFile !== 'object' || manifestFile === null) {
      errors.push('Manifest ist falsch')
      return false
    }

    if (!('id' in manifestFile) || typeof manifestFile.id !== 'string')
      errors.push('Keine ID vergeben')

    if (!('name' in manifestFile) || typeof manifestFile.name !== 'string')
      errors.push('Name fehlt')

    if (!('entry' in manifestFile) || typeof manifestFile.entry !== 'string')
      errors.push('Entry fehlerhaft')

    if (!('author' in manifestFile) || typeof manifestFile.author !== 'string')
      errors.push('Author fehlt')

    if (!('url' in manifestFile) || typeof manifestFile.url !== 'string')
      errors.push('Url fehlt')

    if (
      !('version' in manifestFile) ||
      typeof manifestFile.version !== 'string'
    )
      errors.push('Version fehlt')

    if (
      !('permissions' in manifestFile) ||
      typeof manifestFile.permissions !== 'object' ||
      manifestFile.permissions === null
    ) {
      errors.push('Berechtigungen fehlen')
    }

    if (errors.length) throw errors

    /* const permissions = manifestFile.permissions as Partial<IHaexSpaceExtensionManifest["permissions"]>;
    if (
      ("database" in permissions &&
        (typeof permissions.database !== "object" || permissions.database === null)) ||
      ("filesystem" in permissions && typeof permissions.filesystem !== "object") ||
      permissions.filesystem === null
    ) {
      return false;
    } */

    return true
  }

  const preview = ref<ExtensionPreview>()

  const previewManifestAsync = async (extensionPath: string) => {
    // Read file as bytes (works with content URIs on Android)
    const fileBytes = await readFile(extensionPath)

    preview.value = await invoke<ExtensionPreview>('preview_extension', {
      fileBytes: Array.from(fileBytes),
    })
    return preview.value
  }

  const updateDisplayModeAsync = async (
    extensionId: string,
    displayMode: DisplayMode,
  ) => {
    await invoke('update_extension_display_mode', {
      extensionId,
      displayMode,
    })

    // Update local state
    const extension = availableExtensions.value.find(
      (ext) => ext.id === extensionId,
    )
    if (extension) {
      extension.displayMode = displayMode
    }
  }

  /**
   * Compare two semver version strings
   * @returns -1 if a < b, 0 if a == b, 1 if a > b
   */
  const compareVersions = (a: string, b: string): number => {
    const partsA = a.split('.').map(Number)
    const partsB = b.split('.').map(Number)
    for (let i = 0; i < Math.max(partsA.length, partsB.length); i++) {
      const numA = partsA[i] || 0
      const numB = partsB[i] || 0
      if (numA > numB) return 1
      if (numA < numB) return -1
    }
    return 0
  }

  return {
    availableExtensions,
    checkManifest,
    clearPendingInstall,
    compareVersions,
    currentExtension,
    currentExtensionId,
    downloadAndPreviewAsync,
    extensionEntry,
    installAsync,
    installFilesAsync,
    installPendingAsync,
    registerAndInstallFilesAsync,
    //isActive,
    isExtensionInstalledAsync,
    loadExtensionsAsync,
    preview,
    previewManifestAsync,
    registerInDatabaseAsync,
    removeDevExtensionAsync,
    removeExtensionAsync,
    updateDisplayModeAsync,
  }
})

/* const getMimeType = (file: string) => {
  if (file.endsWith('.css')) return 'text/css'
  if (file.endsWith('.js')) return 'text/javascript'
  return 'text/plain'
} */

/* const removeExtensionFromVaultAsync = async (
  id: string | null,
  version: string | null,
) => {
  if (!id)
    throw new Error(
      'Erweiterung kann nicht gelöscht werden. Es keine ID angegeben',
    )

  if (!version)
    throw new Error(
      'Erweiterung kann nicht gelöscht werden. Es wurde keine Version angegeben',
    )

  const { currentVault } = useVaultStore()
  const removedExtensions = await currentVault?.drizzle
    .delete(haexExtensions)
    .where(and(eq(haexExtensions.id, id), eq(haexExtensions.version, version)))
  return removedExtensions
} */

/* const removeExtensionFilesAsync = async (
  id: string | null,
  version: string | null,
) => {
  try {
    const { getExtensionPathAsync } = useExtensionsStore()
    if (!id)
      throw new Error(
        'Erweiterung kann nicht gelöscht werden. Es keine ID angegeben',
      )

    if (!version)
      throw new Error(
        'Erweiterung kann nicht gelöscht werden. Es wurde keine Version angegeben',
      )

    const extensionDirectory = await getExtensionPathAsync(id, version)
    await remove(extensionDirectory, {
      recursive: true,
    })
  } catch (error) {
    console.error('ERROR removeExtensionFilesAsync', error)
    throw new Error(JSON.stringify(error))
  }
} */
