/**
 * Central Logging Store
 *
 * Provides a unified logging interface for the entire application.
 * All debug logging can be controlled from a single place.
 *
 * Usage:
 *   import { createLogger } from '@/stores/logging'
 *   const log = createLogger('SYNC')
 *   log.info('message')
 *   log.debug('only shown when debug is enabled')
 */

export type LogLevel = 'debug' | 'info' | 'warn' | 'error'

export interface LoggerConfig {
  /** Global debug mode - when false, debug() calls are suppressed */
  debugEnabled: boolean
  /** Per-module debug overrides (module name -> enabled) */
  moduleDebug: Record<string, boolean>
  /** Suppress all logging during bulk operations */
  bulkOperationMode: boolean
  /** Count of suppressed logs during bulk mode (for diagnostics) */
  suppressedCount: number
}

const config = reactive<LoggerConfig>({
  debugEnabled: false,
  moduleDebug: {},
  bulkOperationMode: false,
  suppressedCount: 0,
})

/**
 * Enable/disable global debug mode
 */
export const setDebugEnabled = (enabled: boolean): void => {
  config.debugEnabled = enabled
}

/**
 * Enable/disable debug for a specific module
 */
export const setModuleDebug = (module: string, enabled: boolean): void => {
  config.moduleDebug[module] = enabled
}

/**
 * Enter bulk operation mode - suppresses most logging
 */
export const enterBulkMode = (): void => {
  config.bulkOperationMode = true
  config.suppressedCount = 0
}

/**
 * Exit bulk operation mode
 * Returns the count of suppressed logs
 */
export const exitBulkMode = (): number => {
  config.bulkOperationMode = false
  const count = config.suppressedCount
  if (count > 0) {
    console.log(`[Logging] Exited bulk mode, suppressed ${count} log entries`)
  }
  config.suppressedCount = 0
  return count
}

/**
 * Check if we're in bulk operation mode
 */
export const isInBulkMode = computed(() => config.bulkOperationMode)

/**
 * Get current logging configuration (for debugging)
 */
export const getLoggingConfig = (): Readonly<LoggerConfig> => config

export interface Logger {
  info: (...args: unknown[]) => void
  warn: (...args: unknown[]) => void
  error: (...args: unknown[]) => void
  debug: (...args: unknown[]) => void
}

/**
 * Create a logger instance for a specific module
 *
 * @param module - Module name (e.g., 'SYNC', 'SCANNER', 'EVENTS')
 * @returns Logger instance with info, warn, error, debug methods
 */
export const createLogger = (module: string): Logger => {
  const prefix = `[${module}]`

  const isDebugEnabled = (): boolean => {
    // Module-specific override takes precedence
    if (module in config.moduleDebug) {
      return config.moduleDebug[module]!
    }
    // Fall back to global setting
    return config.debugEnabled
  }

  const shouldSuppress = (level: LogLevel): boolean => {
    // Never suppress errors
    if (level === 'error') return false

    // In bulk mode, suppress info and debug (keep warnings)
    if (config.bulkOperationMode && (level === 'info' || level === 'debug')) {
      config.suppressedCount++
      return true
    }

    return false
  }

  return {
    info: (...args: unknown[]) => {
      if (shouldSuppress('info')) return
      console.log(prefix, ...args)
    },

    warn: (...args: unknown[]) => {
      if (shouldSuppress('warn')) return
      console.warn(prefix, ...args)
    },

    error: (...args: unknown[]) => {
      // Errors are never suppressed
      console.error(prefix, ...args)
    },

    debug: (...args: unknown[]) => {
      if (!isDebugEnabled()) return
      if (shouldSuppress('debug')) return
      console.log(`${prefix} [DEBUG]`, ...args)
    },
  }
}

/**
 * Composable for Vue components
 */
export const useLogging = () => {
  return {
    createLogger,
    setDebugEnabled,
    setModuleDebug,
    enterBulkMode,
    exitBulkMode,
    isInBulkMode,
    getLoggingConfig,
  }
}
