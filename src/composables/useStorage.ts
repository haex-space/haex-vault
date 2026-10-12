/**
 * Storage connections in the settings (spec 038, contracts/tauri-commands.md): the overview with
 * live updates and the texts of the storage errors. No answer of these commands carries a secret.
 */
import { invoke } from '@tauri-apps/api/core'
import type { ConnectionView } from '@bindings/ConnectionView'
import type { CredentialsState } from '@bindings/CredentialsState'
import type { StorageOverview } from '@bindings/StorageOverview'
import type { TestOutcome } from '@bindings/TestOutcome'

/** The tables whose changes (also from another device) change the overview. */
const TABLES = [
  'haex_storage_connections',
  'haex_storages',
  'storage_tests_no_sync',
  'haex_passwords_item_details',
  'extension_permissions',
] as const

/** The overview, read on mount and again when one of its tables changes. */
export function useStorageOverview() {
  const { errString } = useErrorString()
  const overview = ref<StorageOverview | null>(null)
  const failure = ref<string | null>(null)

  async function loadAsync() {
    try {
      overview.value = await invoke<StorageOverview>('storage_list')
      failure.value = null
    } catch (error) {
      failure.value = errString(error)
    }
  }

  onMounted(loadAsync)
  onVaultTablesChanged(TABLES, loadAsync)

  const connection = (id: string): ConnectionView | undefined =>
    overview.value?.connections.find((c) => c.id === id)

  return { overview, failure, loadAsync, connection }
}

type StorageError = {
  kind?: string
  outcome?: TestOutcome
  leftoverKey?: string | null
  field?: string
  state?: CredentialsState
}

/** Whether a storage command failed because the folder of a WebDAV or SFTP storage is missing
 * (spec 050 FR-006); the form then offers to make it. */
export function isFolderMissing(error: unknown): boolean {
  const e = (error ?? {}) as StorageError
  return e.kind === 'StorageTestFailed' && e.outcome === 'folderMissing'
}

/** The text of an error of the storage commands; other errors as `useErrorString` has them. */
export function useStorageError() {
  const { t } = useI18n()
  const { errString } = useErrorString()
  return (error: unknown): string => {
    const e = (error ?? {}) as StorageError
    switch (e.kind) {
      case 'StorageTestFailed': {
        const text = t(`settings.storage.outcome.${e.outcome}`)
        return e.leftoverKey
          ? `${text}. ${t('settings.storage.leftover', { key: e.leftoverKey })}`
          : text
      }
      case 'StorageInvalid':
        return t(`settings.storage.invalid.${e.field}`)
      case 'StorageCredentialsUnavailable':
        return t(`settings.storage.credentialsUnavailable.${e.state}`)
      case 'StorageNotFound':
        return t('settings.storage.notFound')
      default:
        return errString(error)
    }
  }
}
