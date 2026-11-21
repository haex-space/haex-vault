import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'

import {
  allows,
  createBucket,
  createLogState,
  messageText,
  type UiLevel,
} from '~/lib/diagnosticLog'
import type { DiagnosticLogStatus } from '~/types/bindings/DiagnosticLogStatus'

const EVENT_CHANGED = 'haex:diagnostic-log-changed'
const CONSOLE_LEVELS: readonly UiLevel[] = ['error', 'warn', 'info', 'debug']

/**
 * The interface's side of the diagnostic log (spec 098, US2): warnings, errors, uncaught errors,
 * rejected promises and errors of the view layer go to the vault's log as text and stack, with
 * the origin `ui`. `console.info` and `console.debug` only while "Ausführlich" is on. The
 * original `console` call still runs.
 *
 * The plugin loads before any vault is unlocked, so it starts at "Standard" without a session
 * and sends nothing until the backend's event of an unlock names the session; every message
 * carries that session's token, so the backend drops one that outlived its vault (FR-007).
 */
export default defineNuxtPlugin((nuxtApp) => {
  const state = createLogState()
  const bucket = createBucket(() => performance.now())
  let sending = false

  /** Sends one message; the log is best effort, so a failure is dropped and never logged again. */
  const post = (level: UiLevel, message: string) => {
    const { sessionId } = state.current()
    if (!sessionId || sending) return
    sending = true
    try {
      invoke('diagnostic_log_ui_message', { sessionId, level, message }).catch(
        () => {},
      )
    } catch {
      // No IPC (a plain browser): nothing to send to.
    } finally {
      sending = false
    }
  }

  const report = (level: UiLevel, args: readonly unknown[]) => {
    try {
      if (!allows(level, state.current().verbose)) return
      if (!state.current().sessionId || !bucket.take()) return
      post(level, messageText(args))
    } catch {
      // Reporting must never throw into the code that logged.
    }
  }

  for (const level of CONSOLE_LEVELS) {
    const original = console[level].bind(console)
    console[level] = (...args: unknown[]) => {
      original(...args)
      report(level, args)
    }
  }

  window.addEventListener('error', (event) => {
    report('error', [
      event.error ?? event.message,
      ...(event.error
        ? []
        : [`at ${event.filename}:${event.lineno}:${event.colno}`]),
    ])
  })
  window.addEventListener('unhandledrejection', (event) => {
    report('error', ['Unhandled rejection:', event.reason])
  })
  // Nuxt owns `vueApp.config.errorHandler`; its hooks are the way to hear view errors.
  nuxtApp.hook('vue:error', (error, _instance, info) => {
    report('error', ['Vue error:', error, info])
  })
  nuxtApp.hook('app:error', (error) => {
    report('error', ['App error:', error])
  })

  // The one line about a flood: how many messages the last second dropped (FR-014).
  setInterval(() => {
    const dropped = bucket.report()
    if (dropped > 0)
      post(
        'warn',
        `${dropped} messages of the interface were dropped (too many at once)`,
      )
  }, 1000)

  // Listen first, then read: an unlock between the two cannot be missed, and its event wins over
  // the read's older answer. Before the first unlock the read fails with "no active vault".
  void listen<DiagnosticLogStatus>(EVENT_CHANGED, (event) => {
    state.onEvent(event.payload)
  })
    .then(() => invoke<DiagnosticLogStatus>('diagnostic_log_get'))
    .then((status) => state.onStartup(status))
    .catch(() => {})
})
