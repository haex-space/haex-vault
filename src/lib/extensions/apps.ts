// Installed extensions as apps of the window manager (spec 017, US1, T043). Pure: the extensions
// store feeds it, `scripts/check-extensions-apps.ts` tests it.
import { WM_APPS, type AppDefinition } from '../wm/apps.ts'
import type { WmWindow } from '../wm/types.ts'
import type { ExtensionSummary } from '../../types/bindings/ExtensionSummary.ts'

const PREFIX = 'extension.'

/** The app id of an extension's tabs. */
export function extensionAppId(extensionId: string): string {
  return PREFIX + extensionId
}

/** The extension an app id names, or `null` for a haex-vault app. */
export function extensionIdOf(appId: string): string | null {
  return appId.startsWith(PREFIX) ? appId.slice(PREFIX.length) : null
}

/** States on this device in which an extension cannot open (data-model.md §Zustände). */
const UNAVAILABLE = new Set([
  'transferring',
  'signature_failed',
  'migration_failed',
])

/** The i18n key of a device state. */
export function statusKey(status: string): string {
  return `extensions.status.${status}`
}

/** The i18n key of the error kind of a device state, or `null` without one: a broken signature
 * names the rule the bundle broke, a failed migration what went wrong. */
export function statusErrorKey(
  status: string,
  error: string | null | undefined,
): string | null {
  if (!error) return null
  return status === 'signature_failed'
    ? `extensions.install.errors.${error}`
    : `extensions.statusError.${error}`
}

/** One app per installed and enabled extension, with its own name and icon. */
export function extensionApps(
  extensions: readonly ExtensionSummary[],
  icons: Readonly<Record<string, string>>,
): AppDefinition[] {
  return extensions
    .filter((e) => e.state === 'installed' && e.enabled)
    .map((e) => ({
      id: extensionAppId(e.id),
      titleKey: 'wm.apps.extension',
      title: e.title,
      ...(e.description ? { description: e.description } : {}),
      icon: 'lucide:puzzle',
      ...(icons[e.id] ? { iconUrl: icons[e.id] } : {}),
      defaultSize: { width: 960, height: 640 },
      minSize: { width: 360, height: 320 },
      multiInstance: !e.singleInstance,
      tabTitle: 'app' as const,
      ...(e.statusHere && UNAVAILABLE.has(e.statusHere)
        ? { unavailableKey: statusKey(e.statusHere) }
        : {}),
      ...(e.dev ? { dev: true } : {}),
    }))
}

/** The tabs of extensions that no longer run in this vault, disabled or removed on any own device
 * (FR-007, FR-039): they close. A tab of an extension that is only not ready here stays. */
export function tabsOfStoppedExtensions(
  windows: readonly WmWindow[],
  extensions: readonly ExtensionSummary[],
): { windowId: string; tabId: string }[] {
  const running = new Set(
    extensions
      .filter((e) => e.state === 'installed' && e.enabled)
      .map((e) => e.id),
  )
  return windows.flatMap((window) =>
    window.tabs
      .filter((tab) => {
        const id = extensionIdOf(tab.appId)
        return id !== null && !running.has(id)
      })
      .map((tab) => ({ windowId: window.id, tabId: tab.id })),
  )
}

/** The first tab of an extension in window order, or `null` without one (a notification click
 * brings it forward, spec 017 US8). */
export function tabOfExtension(
  windows: readonly WmWindow[],
  extensionId: string,
): { windowId: string; tabId: string } | null {
  const appId = extensionAppId(extensionId)
  for (const window of windows) {
    const tab = window.tabs.find((t) => t.appId === appId)
    if (tab) return { windowId: window.id, tabId: tab.id }
  }
  return null
}

/** haex-vault's own apps followed by the extension apps. */
export function allApps(
  extensionAppList: readonly AppDefinition[],
): AppDefinition[] {
  return [...WM_APPS, ...extensionAppList]
}
