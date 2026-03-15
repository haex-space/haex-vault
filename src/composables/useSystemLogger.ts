import { invoke } from '@tauri-apps/api/core'

type LogMetadata = Record<string, unknown>

export function useSystemLogger(source: string) {
  const deviceStore = useDeviceStore()

  const log = (level: string, message: string, metadata?: LogMetadata) => {
    invoke('log_write_system', {
      level,
      source,
      message,
      metadata: metadata ?? null,
      deviceId: deviceStore.deviceId ?? 'unknown',
    }).catch((err) => {
      console.warn(`[Logger] Failed to write log: ${err}`)
    })
  }

  return {
    debug: (message: string, metadata?: LogMetadata) => log('debug', message, metadata),
    info: (message: string, metadata?: LogMetadata) => log('info', message, metadata),
    warn: (message: string, metadata?: LogMetadata) => log('warn', message, metadata),
    error: (message: string, metadata?: LogMetadata) => log('error', message, metadata),
  }
}
