// Export all handler functions
export { handleDatabaseMethodAsync } from './database'
export { handleFilesystemMethodAsync } from './filesystem'
export { handleWebMethodAsync } from './web'
export { handlePermissionsMethodAsync } from './permissions'
export { handleContextMethodAsync, setContextGetters, isContextGettersInitialized } from './context'
export { handleWebStorageMethodAsync } from './webStorage'
export { handleRemoteStorageMethodAsync } from './remoteStorage'
export { handleLocalSendMethodAsync } from './localsend'

// Export shared types
export type { ExtensionRequest, ExtensionInstance } from './types'
