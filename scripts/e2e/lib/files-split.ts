// Helpers of the split view scenarios (spec 062): two panels repeat the `data-testid`s of the file
// browser (`files-entry-<name>`, `files-up`, `files-search`), so every selector is scoped to the panel
// by its `data-files-panel` attribute. The hooks these return start with `[`, which the rig reads as
// a selector (`instance.click`, `waitForDisplayed`, `doubleClick`).
import type { ScenarioContext } from './scenario.ts'

type Instance = Awaited<ReturnType<ScenarioContext['startInstance']>>

export type PanelSide = 'left' | 'right'

/** The hook of an element with `testId` inside the panel on `side`. */
export const inPanel = (side: PanelSide, testId: string) =>
  `[data-files-panel="${side}"] [data-testid="${testId}"]`

/** The hook of the entry `name` in the panel on `side`. */
export const entryIn = (side: PanelSide, name: string) =>
  inPanel(side, `files-entry-${name}`)

/** How many panels are mounted and visible. */
export const visiblePanels = (instance: Instance) =>
  instance.exec<number>(
    `return [...document.querySelectorAll('[data-files-panel]')]
       .filter((panel) => panel.getClientRects().length > 0).length`,
  )

/** Whether the panel on `side` lists the entry `name`. */
export const panelShows = (instance: Instance, side: PanelSide, name: string) =>
  instance.exec<boolean>(
    `return Boolean(document.querySelector(arguments[0]))`,
    [entryIn(side, name)],
  )

/** The side of the panel that is the active one (`data-active`), `null` when none is. */
export const activePanel = (instance: Instance) =>
  instance.exec<PanelSide | null>(
    `const panel = document.querySelector('[data-files-panel][data-active="true"]')
     return panel ? panel.getAttribute('data-files-panel') : null`,
  )

/** The side of the panel whose list has the focus, `null` when none has. */
export const focusedPanel = (instance: Instance) =>
  instance.exec<PanelSide | null>(
    `const panel = document.activeElement && document.activeElement.closest('[data-files-panel]')
     return panel ? panel.getAttribute('data-files-panel') : null`,
  )

/** Where the focus is, for a failure message: the panel, the element and the active panel. */
export const describeFocus = (instance: Instance) =>
  instance.exec<string>(
    `const el = document.activeElement
     if (!el) return 'nothing'
     const panel = el.closest('[data-files-panel]')
     const active = document.querySelector('[data-files-panel][data-active="true"]')
     return (panel ? panel.getAttribute('data-files-panel') : 'outside') + ' <' + el.tagName.toLowerCase() +
       (el.getAttribute('data-testid') ? ' ' + el.getAttribute('data-testid') : '') +
       (el.hasAttribute('data-files-list') ? ' list' : '') + '> active=' +
       (active ? active.getAttribute('data-files-panel') : 'none')`,
  )

/** The names of the entries in the panel on `side`, in the order shown. */
export const namesIn = (instance: Instance, side: PanelSide) =>
  instance.exec<string[]>(
    `return [...document.querySelectorAll(arguments[0])]
       .map((row) => row.getAttribute('data-path').split(/[\\\\/]/).pop())`,
    [`[data-files-panel="${side}"] [data-files-list] [data-path]`],
  )

/** Puts the focus into the list of the panel on `side`, as a click on its empty area or Tab does. */
export const focusList = (instance: Instance, side: PanelSide) =>
  instance.exec<boolean>(
    `const list = document.querySelector(arguments[0])
     if (!list) return false
     list.focus()
     return true`,
    [`[data-files-panel="${side}"] [data-files-list]`],
  )

/** Whether a toast shows text that matches `pattern` (a regular expression source). */
export const toastShows = (instance: Instance, pattern: string) =>
  instance.exec<boolean>(
    `const pattern = new RegExp(arguments[0], 'i')
     return [...document.querySelectorAll('[data-sonner-toast]')].some((toast) =>
       pattern.test(toast.textContent || ''))`,
    [pattern],
  )
