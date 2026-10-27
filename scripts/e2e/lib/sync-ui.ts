// What a scenario reads and does in the device view of the settings ("Föderation") and in the server
// lists, through the hooks the interface has (contracts/settings-app.md, spec 024). Scenario-facing:
// no platform specifics, only the page.
import { waitForWorkspace } from './flows.ts'
import type { FlowInstance } from './flows.ts'
import type { Device } from './group.ts'
import {
  openSettings,
  runAction,
  settingsLocation,
  waitForLocation,
  wmSnapshot,
} from './settings.ts'

/** A row of the device list as the person sees it. */
export interface DeviceRowView {
  title: string
  current: boolean
  role: string | undefined
  online: boolean
  /** The status text of another device ("online", "zuletzt online vor 3 Min."); null on the own row. */
  status: string | null
}

/** Opens the category "Föderation" of the settings and waits for the device list. */
export async function openFederation(page: FlowInstance): Promise<void> {
  if ((await settingsLocation(page)) === null) await openSettings(page)
  await page.click('settings-category-federation')
  await waitForLocation(page, 'federation')
  await page.waitForDisplayed('settings-device')
}

/** The rows of the device list that are on screen now. */
export function deviceRows(page: FlowInstance): Promise<DeviceRowView[]> {
  return page.exec<DeviceRowView[]>(
    `return [...document.querySelectorAll('[data-testid="settings-device"]')].map((row) => ({
       title: row.querySelector('span, label').textContent.trim(),
       current: row.dataset.current === 'true',
       role: row.dataset.role,
       online: row.dataset.online === 'true',
       status: row.querySelector('[data-testid="settings-device-status"]')?.textContent.trim() ?? null,
     }))`,
  )
}

/** Whether an element with the hook is on the page at all. */
export function exists(page: FlowInstance, hook: string): Promise<boolean> {
  return page.exec<boolean>(
    `return document.querySelector(arguments[0]) !== null`,
    [`[data-testid="${hook}"]`],
  )
}

/** The npub the device view shows. */
export function shownNpub(page: FlowInstance): Promise<string> {
  return page.exec<string>(
    `return document.querySelector('[data-testid="settings-identity-npub"]').textContent.trim()`,
  )
}

/** Whether the npub can be typed into: it or an ancestor is an input, a text area or editable. */
export function npubIsEditable(page: FlowInstance): Promise<boolean> {
  return page.exec<boolean>(
    `const el = document.querySelector('[data-testid="settings-identity-npub"]')
     return el.isContentEditable || ['INPUT', 'TEXTAREA'].includes(el.tagName) || el.closest('input, textarea, [contenteditable="true"]') !== null`,
  )
}

/**
 * Clicks the toggle of every server of one kind (`default` for the built-in ones, `added`) in a list
 * whose test id prefix is `testId` (`link-servers-nostr`, `settings-servers-nostrRelays`) and that is
 * in the wanted state, one after the other. Returns how many it clicked.
 */
export async function toggleServers(
  page: FlowInstance,
  testId: string,
  kind: 'default' | 'added',
  from: 'on' | 'off',
): Promise<number> {
  return page.exec<number>(
    `const rows = [...document.querySelectorAll('[data-testid="' + arguments[0] + '-' + arguments[1] + '"][data-enabled="' + arguments[2] + '"]')]
     rows.forEach((row) => row.querySelector('[data-testid="' + arguments[0] + '-toggle"]').click())
     return rows.length`,
    [testId, kind, from === 'on' ? 'true' : 'false'],
  )
}

/** Switches off every built-in server of a list that is in use. */
export function switchOffDefaultServers(
  page: FlowInstance,
  testId: string,
): Promise<number> {
  return toggleServers(page, testId, 'default', 'on')
}

/** Adds a server URL to a list through its input. */
export async function addServer(
  page: FlowInstance,
  testId: string,
  url: string,
): Promise<void> {
  await page.type(`${testId}-input`, url)
  await page.click(`${testId}-add`)
}

/** Counts the rows of a list: `default`, `added`, with the `data-enabled` state if given. */
export function serverRows(
  page: FlowInstance,
  testId: string,
  kind: 'default' | 'added',
  enabled?: boolean,
): Promise<number> {
  return page.exec<number>(
    `return document.querySelectorAll('[data-testid="' + arguments[0] + '-' + arguments[1] + '"]' + (arguments[2] === null ? '' : '[data-enabled="' + arguments[2] + '"]')).length`,
    [testId, kind, enabled ?? null],
  )
}

/** The link view of a main device shows a code (a link that was done before offers to start again). */
export async function showLinkCode(page: FlowInstance): Promise<string> {
  const open = async () => {
    const outcome = await runAction(page, 'settings.devices.link')
    if (!outcome.ok)
      throw new Error(`settings.devices.link: ${JSON.stringify(outcome)}`)
    return exists(page, 'link-device-view')
  }
  const end = Date.now() + 10_000
  while (!(await open())) {
    if (Date.now() > end) throw new Error('the link view did not open')
    await new Promise((resolve) => setTimeout(resolve, 100))
  }
  if (await exists(page, 'link-again')) await page.click('link-again')
  await page.click('link-show-code')
  await page.waitForDisplayed('link-code-text')
  return page.exec<string>(
    `return document.querySelector('[data-testid="link-code-text"]').textContent.trim()`,
  )
}

export interface FormLink {
  code: string
  deviceName: string
  vaultName: string
  passphrase: string
  /** The only Nostr server: the built-in ones are switched off and this one is added. */
  relayUrl: string
  /** The only iroh-Relays, as for the group's other devices: the built-in ones are switched off. */
  irohRelays: string[]
}

/**
 * The form "Mit einer Vault verknüpfen" of the start page, filled as a person does it: the code, a name
 * for the device and for the vault, a passphrase twice, and the Nostr server of the test.
 */
export async function submitLinkForm(
  page: FlowInstance,
  link: FormLink,
): Promise<void> {
  await page.click('landing-link')
  await page.waitForDisplayed('link-code')
  await page.type('link-code', link.code)
  await page.type('link-device-name', link.deviceName)
  await page.type('link-vault-name', link.vaultName)
  await page.type('#link-passphrase', link.passphrase)
  await page.type('#link-passphrase-confirm', link.passphrase)
  await addServer(page, 'link-servers-nostr', link.relayUrl)
  await switchOffDefaultServers(page, 'link-servers-nostr')
  for (const url of link.irohRelays)
    await addServer(page, 'link-servers-iroh', url)
  await switchOffDefaultServers(page, 'link-servers-iroh')
  await page.click('link-submit')
}

/** The state the link form reports while it joins (`data-state` of `link-progress`). */
export function linkProgress(page: FlowInstance): Promise<string | null> {
  return page.exec<string | null>(
    `return document.querySelector('[data-testid="link-progress"]')?.dataset.state ?? null`,
  )
}

/** After a link: open the new vault from the list with its passphrase and wait for its workspace. */
export async function openLinkedVault(
  page: FlowInstance,
  passphrase: string,
): Promise<void> {
  await page.click('link-open-vault')
  await page.waitForDisplayed('#unlock-passphrase')
  await page.type('#unlock-passphrase', passphrase)
  await page.click('[type="submit"][form="unlock-form"]')
  await waitForWorkspace(page)
}

/** The titles the chat's thread list shows (hook `chat-thread`), in the order shown. */
export function chatTitles(page: FlowInstance): Promise<string[]> {
  return page.exec<string[]>(
    `return [...document.querySelectorAll('[data-testid="chat-thread"]')].map((row) => row.querySelector('span.truncate')?.textContent.trim() ?? '')`,
  )
}

/** Starts a new chat and sends the text as its first message, through the composer. */
export async function sendInNewChat(
  page: FlowInstance,
  text: string,
): Promise<void> {
  await page.click('chat-new')
  await sendInChat(page, text)
}

/** Types the text into the composer and sends it. */
export async function sendInChat(
  page: FlowInstance,
  text: string,
): Promise<void> {
  await page.type('chat-input', text)
  await page.click('chat-send')
}

/** Whether the switch "Sitzung wiederherstellen" in the general settings is on. */
export function sessionRestoreOn(page: FlowInstance): Promise<boolean> {
  return page.exec<boolean>(
    `return document.querySelector('[data-testid="session-restore-switch"]')?.getAttribute('data-state') === 'checked'`,
  )
}

/**
 * Brings an app to the front through the window manager: an app that already has a tab is activated,
 * otherwise it is opened. Opening again would stack a second window over the first (chat can have
 * several), and its controls would sit under the other one.
 */
export async function showApp(
  page: FlowInstance,
  appId: string,
): Promise<void> {
  await waitForMounted(page)
  const tab = (await wmSnapshot(page)).windows
    .flatMap((window) => window.tabs.filter((t) => t.appId === appId))
    .at(0)
  const outcome =
    tab === undefined
      ? await runAction(page, 'wm.app.open', { appId })
      : await runAction(page, 'wm.tab.activate', { tabId: tab.id })
  if (!outcome.ok) {
    throw new Error(`showing ${appId}: ${JSON.stringify(outcome)}`)
  }
}

/**
 * Waits until the page's Vue app is mounted. The path of a freshly opened vault is already
 * `/workspace/…` while the app is still starting, and the window manager cannot be reached before
 * (seen on a slow CI runner: "undefined is not an object (evaluating '….__vue_app__.config')").
 */
async function waitForMounted(page: FlowInstance): Promise<void> {
  const end = Date.now() + 15_000
  while (
    !(await page.exec<boolean>(
      `return Boolean(document.querySelector('#__nuxt')?.__vue_app__)`,
    ))
  ) {
    if (Date.now() > end) throw new Error('the app did not mount')
    await new Promise((resolve) => setTimeout(resolve, 100))
  }
}

/**
 * On the device view of the observer, presses "Entfernen" in the row of the target and waits for the
 * view that explains the consequences. The rows carry no public key, so the row is found by the name
 * the observer lists for the target (names are unique in a group).
 */
export async function openRemoveDevice(
  observer: Pick<Device, 'address' | 'page' | 'deviceList'>,
  target: Pick<Device, 'address' | 'pubkey'>,
): Promise<void> {
  const pubkey = await target.pubkey()
  const alias = (await observer.deviceList()).find(
    (row) => row.devicePubkey === pubkey,
  )?.alias
  if (alias === null || alias === undefined) {
    throw new Error(
      `${observer.address} lists no name for ${target.address}, its row cannot be found`,
    )
  }
  await openFederation(observer.page)
  const pressed = await observer.page.exec<boolean>(
    `const row = [...document.querySelectorAll('[data-testid="settings-device"]')]
       .find((r) => r.querySelector('span, label').textContent.trim() === arguments[0])
     const button = row?.querySelector('[data-testid="settings-device-remove"]')
     button?.click()
     return button != null`,
    [alias],
  )
  if (!pressed) {
    throw new Error(
      `${observer.address} offers no "Entfernen" for ${target.address} ("${alias}")`,
    )
  }
  await observer.page.waitForDisplayed('remove-device-view')
}

/** The consequences the remove view lists, one entry each. */
export function removeConsequences(page: FlowInstance): Promise<string[]> {
  return page.exec<string[]>(
    `return [...document.querySelectorAll('[data-testid="remove-device-view"] li li')].map((li) => li.textContent.trim())`,
  )
}

/** Confirms the removal and waits until the view is left. */
export async function confirmRemoveDevice(page: FlowInstance): Promise<void> {
  await page.click('remove-device-confirm')
  const end = Date.now() + 15_000
  while (await exists(page, 'remove-device-view')) {
    if (Date.now() > end) throw new Error('the remove view did not close')
    await new Promise((resolve) => setTimeout(resolve, 100))
  }
}

/** The notice a device without a place in the vault shows at the top of the device view, or null. */
export function federationNotice(page: FlowInstance): Promise<string | null> {
  return page.exec<string | null>(
    `return document.querySelector('[data-testid="settings-federation-notice"]')?.textContent.trim() ?? null`,
  )
}

/** How many requests of copies to join the device view of a main device lists. */
export function admissionRequestCount(page: FlowInstance): Promise<number> {
  return page.exec<number>(
    `return document.querySelectorAll('[data-testid="admission-request"]').length`,
  )
}

/**
 * On a main device: opens the device view, waits for the one open request of a copy and answers it with
 * "Aufnehmen" or "Ablehnen", then waits until the request is gone from the list.
 */
export async function decideAdmission(
  page: FlowInstance,
  admit: boolean,
): Promise<void> {
  await openFederation(page)
  await page.waitForDisplayed('admission-request')
  const count = await admissionRequestCount(page)
  if (count !== 1) {
    throw new Error(`expected one open request of a copy, found ${count}`)
  }
  await page.click(admit ? 'admission-admit' : 'admission-reject')
  const end = Date.now() + 15_000
  while ((await admissionRequestCount(page)) > 0) {
    if (Date.now() > end) throw new Error('the request stayed in the list')
    await new Promise((resolve) => setTimeout(resolve, 100))
  }
}
