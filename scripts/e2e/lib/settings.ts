// Helpers for the settings scenarios (spec 023-settings-app, quickstart S1–S22). Controls are reached
// by hook (contracts/settings-app.md "Test hooks"); window manager actions run through the page's own
// action runner, the same catalog a user's click or a later agent goes through (spec 020).
import type { FlowInstance } from './flows.ts'

/** The window manager store, reached through the Vue app the page mounted on `#__nuxt`. */
export const WM = `document.querySelector('#__nuxt').__vue_app__.config.globalProperties.$pinia._s.get('windowManager')`

/**
 * The workspace can remount while a vault is being reopened. The mount check and the following
 * window-manager call are separate WebDriver requests, so the app may disappear between them on a
 * slow runner. A window activation can also detach the element while WebKit is dispatching the
 * preceding request. Retry only those transient conditions; all other script failures remain real
 * failures.
 */
async function withMounted<T>(
  instance: FlowInstance,
  action: () => Promise<T>,
): Promise<T> {
  const end = Date.now() + 15_000
  for (;;) {
    try {
      return await action()
    } catch (error) {
      if (
        !(error instanceof Error) ||
        (!error.message.includes('__vue_app__') &&
          !error.message.includes('el.dispatchEvent')) ||
        Date.now() >= end
      ) {
        throw error
      }
    }
    await new Promise((resolve) => setTimeout(resolve, 100))
  }
}

export interface ActionOutcome {
  ok: boolean
  result?: unknown
  error?: unknown
}

/** Runs a catalog action as the user would (spec 020 `runAction`); the sync script's promise is awaited
 * by the driver. */
export async function runAction(
  instance: FlowInstance,
  id: string,
  input: Record<string, unknown> = {},
): Promise<ActionOutcome> {
  return withMounted(instance, () =>
    instance.exec<ActionOutcome>(
      `return ${WM}.runAction(arguments[0], arguments[1]).then((outcome) => JSON.parse(JSON.stringify(outcome)))`,
      [id, input],
    ),
  )
}

/** Tabs and windows as the window manager holds them. */
export interface WmSnapshot {
  windows: Array<{
    id: string
    maximized: boolean
    activeTabId: string
    tabs: Array<{ id: string; appId: string }>
  }>
}

export async function wmSnapshot(instance: FlowInstance): Promise<WmSnapshot> {
  return withMounted(instance, () =>
    instance.exec<WmSnapshot>(
      `const wm = ${WM}; return JSON.parse(JSON.stringify({ windows: wm.windows.map((w) => ({ id: w.id, maximized: w.maximized, activeTabId: w.activeTabId, tabs: w.tabs.map((t) => ({ id: t.id, appId: t.appId })) })) }))`,
    ),
  )
}

export function settingsTabs(snapshot: WmSnapshot) {
  return snapshot.windows.flatMap((w) =>
    w.tabs
      .filter((t) => t.appId === 'system.settings')
      .map((t) => ({ windowId: w.id, tabId: t.id })),
  )
}

/** Turns "Sitzung wiederherstellen" on or off through the settings action, the way the switch does:
 * the window manager takes the setting over and starts saving. (`wm_session_restore_set` alone
 * only stores it; the window manager reads it when it restores the session.) */
export async function setSessionRestore(
  instance: FlowInstance,
  enabled: boolean,
): Promise<void> {
  const outcome = await runAction(instance, 'settings.sessionRestore.set', {
    enabled,
  })
  if (!outcome.ok) {
    throw new Error(
      `settings.sessionRestore.set failed: ${JSON.stringify(outcome)}`,
    )
  }
}

/** The location id of the settings title on screen (`data-location`), or `null`. */
export async function settingsLocation(
  instance: FlowInstance,
): Promise<string | null> {
  return instance.exec<string | null>(
    `const title = [...document.querySelectorAll('[data-testid="settings-title"]')].find((el) => el.offsetParent !== null)
     return title ? title.dataset.location ?? null : null`,
  )
}

/** Polls until the settings show the location, failing with the last one seen. */
export async function waitForLocation(
  instance: FlowInstance,
  id: string,
  deadlineMs = 5000,
): Promise<void> {
  const end = Date.now() + deadlineMs
  for (;;) {
    const last = await settingsLocation(instance)
    if (last === id) return
    if (Date.now() >= end) {
      throw new Error(
        `timed out after ${deadlineMs} ms waiting for the settings location "${id}" (last seen: ${JSON.stringify(last)})`,
      )
    }
    await new Promise((resolve) => setTimeout(resolve, 50))
  }
}

/** Opens the settings from the launcher (S1) and waits for their first category. */
export async function openSettings(instance: FlowInstance): Promise<void> {
  await instance.click('open-launcher')
  await instance.click('[data-app-id="system.settings"]')
  await waitForLocation(instance, 'general')
}

/** Opens a settings select by its hook and picks the option with the value (`data-value`). */
export async function choose(
  instance: FlowInstance,
  selectHook: string,
  value: string,
): Promise<void> {
  await instance.click(selectHook)
  await instance.click(`[role="option"][data-value="${value}"]`)
}

/** Gives the window the rect through `wm.window.setGeometry`. New windows open maximized and a
 * maximized window renders at the full area whatever its stored rect is, so it is restored first. */
export async function setWindowGeometry(
  instance: FlowInstance,
  windowId: string,
  rect: { x: number; y: number; width: number; height: number },
): Promise<void> {
  const window = (await wmSnapshot(instance)).windows.find(
    (w) => w.id === windowId,
  )
  if (window?.maximized) {
    const restored = await runAction(instance, 'wm.window.toggleMaximize', {
      windowId,
    })
    if (!restored.ok) {
      throw new Error(
        `wm.window.toggleMaximize failed: ${JSON.stringify(restored)}`,
      )
    }
  }
  const outcome = await runAction(instance, 'wm.window.setGeometry', {
    windowId,
    ...rect,
  })
  if (!outcome.ok) {
    throw new Error(`wm.window.setGeometry failed: ${JSON.stringify(outcome)}`)
  }
}

/** Resizes the window holding the app's first tab through `wm.window.setGeometry` (FR-004 is about the
 * window's width, not the screen's). */
export async function resizeAppWindow(
  instance: FlowInstance,
  appId: string,
  width: number,
  height = 700,
): Promise<void> {
  const tab = (await wmSnapshot(instance)).windows
    .flatMap((w) =>
      w.tabs.filter((t) => t.appId === appId).map(() => ({ windowId: w.id })),
    )
    .at(0)
  if (!tab) throw new Error(`no ${appId} window to resize`)
  await setWindowGeometry(instance, tab.windowId, {
    x: 20,
    y: 20,
    width,
    height,
  })
}

/** Resizes the window holding the settings. */
export function resizeSettingsWindow(
  instance: FlowInstance,
  width: number,
  height = 700,
): Promise<void> {
  return resizeAppWindow(instance, 'system.settings', width, height)
}

/** Whether the element found by selector is on screen: laid out, visible and not scrolled or slid
 * out of its container. */
export async function isShown(
  instance: FlowInstance,
  selector: string,
): Promise<boolean> {
  return instance.exec<boolean>(
    `const el = document.querySelector(arguments[0])
     if (!el) return false
     const style = getComputedStyle(el)
     if (style.visibility === 'hidden' || style.display === 'none') return false
     const rect = el.getBoundingClientRect()
     if (rect.width < 1 || rect.height < 1) return false
     const clip = el.parentElement?.getBoundingClientRect()
     return !clip || (rect.right > clip.left + 1 && rect.left < clip.right - 1)`,
    [selector],
  )
}

/** WebDriver key codes for `instance.type`. */
export const KEY = {
  enter: '\uE007',
  escape: '\uE00C',
  backspace: '\uE003',
  arrowDown: '\uE015',
  arrowUp: '\uE013',
  arrowLeft: '\uE012',
  arrowRight: '\uE014',
  home: '\uE011',
  end: '\uE010',
  delete: '\uE017',
  f2: '\uE032',
  f5: '\uE035',
  /** Held down for the rest of the text until `release` (WebDriver modifier keys). */
  control: '\uE009',
  shift: '\uE008',
  alt: '\uE00A',
  release: '\uE000',
} as const

/**
 * Empties the device name field with Backspace, the way a person does it, once it shows the stored
 * name: the field fills in after the view mounts, and clearing it earlier would leave the name in.
 */
export async function clearAlias(instance: FlowInstance): Promise<void> {
  await instance.waitForDisplayed('settings-alias')
  const end = Date.now() + 5000
  for (;;) {
    const info = await instance.invoke('current_device_info')
    const stored =
      'ok' in info && info.ok
        ? ((info.data as { alias: string | null }).alias ?? '')
        : null
    const shown = await instance.exec<string>(
      'return document.querySelector(\'[data-testid="settings-alias"]\').value',
    )
    const enabled = await instance.exec<boolean>(
      'return !document.querySelector(\'[data-testid="settings-alias"]\').disabled',
    )
    if (stored !== null && shown === stored && enabled) {
      await instance.type('settings-alias', KEY.backspace.repeat(shown.length))
      return
    }
    if (Date.now() >= end) {
      throw new Error(
        `the name field shows ${JSON.stringify(shown)}, the stored name is ${JSON.stringify(stored)}`,
      )
    }
    await new Promise((resolve) => setTimeout(resolve, 50))
  }
}

/**
 * WCAG contrast ratio of an element's text against what is behind it, both composited on a canvas so
 * any CSS colour syntax (oklch included) and translucent layers count as painted (FR-013, S20).
 */
export async function textContrast(
  instance: FlowInstance,
  selector: string,
): Promise<number> {
  return instance.exec<number>(
    `const el = document.querySelector(arguments[0])
     if (!el) throw new Error('no element for ' + arguments[0])
     ${PAINT}
     const layers = []
     for (let node = el; node; node = node.parentElement) layers.unshift(getComputedStyle(node).backgroundColor)
     const back = paint(['white', ...layers])
     const front = paint(['white', ...layers, getComputedStyle(el).color])
     return ratio(front, back)`,
    [selector],
  )
}

/** Contrast between two elements' painted backgrounds, e.g. a switch track against its card. */
export async function backgroundContrast(
  instance: FlowInstance,
  selector: string,
  againstSelector: string,
): Promise<number> {
  return instance.exec<number>(
    `${PAINT}
     const stack = (sel) => {
       const el = document.querySelector(sel)
       if (!el) throw new Error('no element for ' + sel)
       const layers = []
       for (let node = el; node; node = node.parentElement) layers.unshift(getComputedStyle(node).backgroundColor)
       return paint(['white', ...layers])
     }
     return ratio(stack(arguments[0]), stack(arguments[1]))`,
    [selector, againstSelector],
  )
}

const PAINT = `
  const canvas = document.createElement('canvas')
  canvas.width = canvas.height = 1
  const g = canvas.getContext('2d', { willReadFrequently: true })
  const paint = (colors) => {
    g.clearRect(0, 0, 1, 1)
    for (const color of colors) { g.fillStyle = color; g.fillRect(0, 0, 1, 1) }
    return [...g.getImageData(0, 0, 1, 1).data].slice(0, 3)
  }
  const luminance = (rgb) => {
    const [r, gr, b] = rgb.map((v) => { v /= 255; return v <= 0.03928 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4 })
    return 0.2126 * r + 0.7152 * gr + 0.0722 * b
  }
  const ratio = (a, b) => {
    const [hi, lo] = [luminance(a), luminance(b)].sort((x, y) => y - x)
    return (hi + 0.05) / (lo + 0.05)
  }
`
