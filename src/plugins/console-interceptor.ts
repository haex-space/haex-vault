/**
 * Global Console Interceptor Plugin
 * Captures console messages and writes them to the structured logging system.
 * Uses source 'console' with source_type 'system'.
 * Retention: 1 day (configured via vault_settings).
 */

import { invoke } from '@tauri-apps/api/core'

const originalConsole = {
  log: console.log,
  info: console.info,
  warn: console.warn,
  error: console.error,
  debug: console.debug,
}

// Map console levels to our log levels ('log' maps to 'debug')
const levelMap: Record<string, string> = {
  log: 'debug',
  info: 'info',
  warn: 'warn',
  error: 'error',
  debug: 'debug',
}

// Buffer logs until device ID is available
let deviceId: string | null = null
let bufferedLogs: { level: string; message: string }[] = []
let disabled = false

function flushBuffer() {
  if (!deviceId) return
  for (const log of bufferedLogs) {
    writeLog(log.level, log.message)
  }
  bufferedLogs = []
}

// Prefixes that must not be persisted to DB to prevent sync feedback loops:
// sync logging → interceptor → insert_log → CRDT dirty → push → more sync logging → ∞
const SKIP_PREFIXES = ['[SYNC]', '[SYNC SCANNER]']

function writeLog(level: string, message: string) {
  if (disabled) return

  // Skip sync-related messages to prevent feedback loop with CRDT dirty tracking
  if (SKIP_PREFIXES.some((prefix) => message.startsWith(prefix))) return

  if (!deviceId) {
    bufferedLogs.push({ level, message })
    return
  }

  invoke('log_write_system', {
    level,
    source: 'console',
    message,
    metadata: null,
    deviceId,
  }).catch(() => {
    // Silently fail — don't recurse into console.error
  })
}

function formatArgs(args: unknown[]): string {
  return args
    .map((arg) => {
      if (arg === null) return 'null'
      if (arg === undefined) return 'undefined'
      if (typeof arg === 'object') {
        try {
          return JSON.stringify(arg, null, 2)
        } catch {
          return String(arg)
        }
      }
      return String(arg)
    })
    .join(' ')
}

function interceptConsole(level: 'log' | 'info' | 'warn' | 'error' | 'debug') {
  console[level] = function (...args: unknown[]) {
    originalConsole[level].apply(console, args)
    writeLog(levelMap[level] ?? 'debug', formatArgs(args))
  }
}

export default defineNuxtPlugin(() => {
  interceptConsole('log')
  interceptConsole('info')
  interceptConsole('warn')
  interceptConsole('error')
  interceptConsole('debug')

  originalConsole.log('[HaexSpace] Console interceptor → structured logging')

  return {
    provide: {
      setConsoleLoggerDeviceId: (id: string) => {
        deviceId = id
        disabled = false
        flushBuffer()
      },
      disableConsoleLogger: () => {
        disabled = true
        deviceId = null
        bufferedLogs = []
      },
    },
  }
})
