// Helpers for the extension scenes of spec 017 (quickstart §2–§5): installing a fixture bundle,
// opening it as an app, and talking to the probe page inside its sandboxed frame.
import { fileURLToPath } from 'node:url'
import { unwrap, type FlowInstance } from './flows.ts'
import type { Page } from './page.ts'
import { runAction } from './settings.ts'
import { onDevice } from './platform/device-files.ts'
import {
  privateNetworkAddress as findPrivateNetworkAddress,
  reachFromDevice,
} from './platform/reach.ts'

/** Makes a local server of a scenario reachable from the device under test at the same address. */
export const reachableFromDevice = (port: number): void => reachFromDevice(port)

/** Returns this machine's private-network address for a server used by an extension scene. */
export const privateNetworkAddress = (): string => findPrivateNetworkAddress()

/** A fixture bundle: `vectors/<name>.xt` from the shared vectors or `e2e/<name>.xt`. */
export function fixture(kind: 'vectors' | 'e2e', name: string): string {
  const folder = kind === 'vectors' ? 'extension_bundles' : 'extension_e2e'
  return fileURLToPath(
    new URL(
      `../../../src-tauri/tests/fixtures/${folder}/${name}.xt`,
      import.meta.url,
    ),
  )
}

export interface InstalledExtension {
  id: string
  title: string
}

/** Installs a bundle as the install dialog would, with every declared permission ticked. */
export async function install(
  page: Page,
  path: string,
): Promise<InstalledExtension> {
  return unwrap<InstalledExtension>(
    'extension_install',
    await page.invoke('extension_install', {
      args: { file: onDevice(path, page), accepted: [], confirmed: false },
    }),
  )
}

/** The refusal of a bundle: its install error's reason. */
export async function refusal(page: Page, path: string): Promise<string> {
  const result = (await page.invoke('extension_install', {
    args: { file: onDevice(path, page), accepted: [], confirmed: false },
  })) as { ok: boolean; error?: { kind?: string; reason?: string } }
  if (result.ok) throw new Error(`${path} was installed`)
  return `${result.error?.kind}:${result.error?.reason}`
}

/** Opens an extension from the launcher and waits until its frame is shown (the SDK is ready). */
export async function openFromLauncher(
  instance: FlowInstance,
  extension: InstalledExtension,
): Promise<void> {
  await instance.click('open-launcher')
  await instance.click(`[data-app-id="extension.${extension.id}"]`)
  await instance.waitForDisplayed(
    `[data-extension-id="${extension.id}"]`,
    20_000,
  )
}

/** Starts an extension on this device without showing it (its migrations run then). */
export async function start(
  page: Page,
  extension: InstalledExtension,
): Promise<void> {
  const opened = unwrap<{ frame: string }>(
    'extension_frame_open',
    await page.invoke('extension_frame_open', {
      extensionId: extension.id,
      tabId: 'e2e',
    }),
  )
  unwrap(
    'extension_frame_close',
    await page.invoke('extension_frame_close', { frame: opened.frame }),
  )
}

/** Runs `script` inside the displayed frame of `extension`, then returns to holzi. */
export async function inFrame<T>(
  page: Page,
  extension: InstalledExtension,
  script: string,
  args: unknown[] = [],
): Promise<T> {
  await page.frame(`[data-extension-id="${extension.id}"]`, 20_000)
  try {
    return await page.exec<T>(script, args)
  } finally {
    await page.frame(null)
  }
}

export interface BridgeAnswer {
  id: string
  result?: unknown
  error?: { code: number; message: string; details?: unknown }
}

/** A bridge request of the probe page, answered over holzi's port. */
export async function probeRequest(
  page: Page,
  extension: InstalledExtension,
  method: string,
  params: unknown,
): Promise<BridgeAnswer> {
  return inFrame<BridgeAnswer>(
    page,
    extension,
    `return window.probe.request(arguments[0], arguments[1]).then((a) => JSON.parse(JSON.stringify(a)))`,
    [method, params],
  )
}

/** Goes back in the tab that shows `extension`, as holzi's Back does. */
export async function backInTab(
  instance: FlowInstance,
  extension: InstalledExtension,
): Promise<void> {
  const outcome = await runAction(instance, 'wm.tab.back', {})
  if (!outcome.ok)
    throw new Error(
      `back in the tab of ${extension.title}: ${JSON.stringify(outcome)}`,
    )
}
