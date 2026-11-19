import { invoke } from '@tauri-apps/api/core'

import { runPlatformProbe } from '~/lib/platformProbe'
import { runMediaProbe, type ProbeMedia } from '~/lib/platformProbeMedia'

declare global {
  interface Window {
    __HAEX_PROBE__?: { port: number; media?: ProbeMedia }
  }
}

/**
 * The CI's platform probe (spec 044, T007, T041): runs only when Rust handed over a port, plays and
 * draws the media files Rust serves, and reports through IPC, which no network rule can block.
 */
export default defineNuxtPlugin(() => {
  const probe = window.__HAEX_PROBE__
  if (!probe) return
  const violations: string[] = []
  document.addEventListener('securitypolicyviolation', (event) => {
    violations.push(`${event.effectiveDirective} ${event.blockedURI}`)
  })
  void runPlatformProbe(
    probe.port,
    window.fetch.bind(window),
    navigator.userAgent,
    {
      origin: window.location.origin,
      secure: window.isSecureContext,
      violations: () => [...violations],
    },
  )
    .then(async (report) => {
      if (probe.media && report.steps.every((step) => step.ok))
        report.steps.push(...(await runMediaProbe(probe.media)))
      return invoke('platform_probe_report', { report })
    })
    .catch((error) => {
      console.error('platform probe report failed', error)
    })
})
