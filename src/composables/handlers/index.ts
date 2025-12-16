// Export all handler functions
export { handleDatabaseMethodAsync } from './database'
export { handleFilesystemMethodAsync } from './filesystem'
export { handleFileSyncMethodAsync } from './filesync'
export { handleWebMethodAsync } from './web'
export { handlePermissionsMethodAsync } from './permissions'
export { handleContextMethodAsync, setContextGetters } from './context'
export { handleStorageMethodAsync } from './storage'

// Export shared types
export type { ExtensionRequest, ExtensionInstance } from './types'
