// Helpers for the password manager scenarios (spec 034, T092): opening the window and reading what the
// backend holds through its commands, so a scenario checks data, not pixels. Entries are made through
// `passwords_create_item` where the editor is not what is being tested.
import { deviceFiles } from './extension-files.ts'
import { unwrap } from './flows.ts'
import { toSelector } from './page.ts'
import { resizeAppWindow } from './settings.ts'
import type { FlowInstance } from './flows.ts'
import type { WaitContext } from './sync-flows.ts'

/** The overview as `passwords_load_overview` returns it, reduced to what the scenarios read. */
export interface Overview {
  headers: Array<{
    id: string
    title: string | null
    groupId: string | null
    tags: Array<{ id: string; name: string }>
  }>
  tags: Array<{ id: string; name: string; itemCount: number }>
  groups: Array<{ id: string; name: string | null; parentId: string | null }>
}

/** Opens the password manager from the launcher, fits its window into the virtual screen (the
 * default geometry is wider than the 800 px screen, so the right-hand buttons would lie outside it)
 * and waits for its toolbar. */
export async function openPasswords(instance: FlowInstance): Promise<void> {
  await instance.click('open-launcher')
  await instance.click('[data-app-id="system.passwords"]')
  await instance.waitForDisplayed('passwords-search')
  await resizeAppWindow(instance, 'system.passwords', 760, 560)
  await instance.waitForDisplayed('passwords-new')
}

/** What the backend holds right now. */
export async function overview(
  instance: Pick<FlowInstance, 'invoke'>,
): Promise<Overview> {
  return unwrap<Overview>(
    'passwords_load_overview',
    await instance.invoke('passwords_load_overview'),
  )
}

/** Creates an entry through the command (in a folder when `groupId` is given) and returns its id. */
export async function createEntry(
  instance: Pick<FlowInstance, 'invoke'>,
  input: Record<string, unknown>,
  groupId?: string,
): Promise<string> {
  const result = unwrap<{ itemId: string }>(
    'passwords_create_item',
    await instance.invoke('passwords_create_item', {
      args: groupId ? { input, groupId } : { input },
    }),
  )
  return result.itemId
}

/** Creates a folder through the command and returns its id. */
export async function createFolder(
  instance: Pick<FlowInstance, 'invoke'>,
  name: string,
  parentId?: string,
): Promise<string> {
  return unwrap<{ groupId: string }>(
    'passwords_create_group',
    await instance.invoke('passwords_create_group', {
      args: parentId ? { name, parentId } : { name },
    }),
  ).groupId
}

/** Imports an export file of another password manager through the command: the text is written to
 * a fresh folder of the device, which is removed again once the import has run. */
export async function importExport(
  instance: Pick<FlowInstance, 'invoke'>,
  source: string,
  fileName: string,
  text: string,
): Promise<void> {
  const files = deviceFiles('e2e-passwords-import-')
  try {
    files.write(fileName, text)
    unwrap(
      'passwords_import_run',
      await instance.invoke('passwords_import_run', {
        args: { source, file: files.path(fileName), onDuplicate: 'create' },
      }),
    )
  } finally {
    files.remove()
  }
}

/** Attaches files to an entry through the command, as the file dialog would: each is written to a
 * fresh folder of the device, which is removed again afterwards. Returns the attachment ids in
 * order. */
export async function addAttachments(
  instance: Pick<FlowInstance, 'invoke'>,
  itemId: string,
  files: readonly { name: string; content: string | Uint8Array }[],
): Promise<string[]> {
  const folder = deviceFiles('e2e-passwords-attachments-')
  try {
    const ids: string[] = []
    for (const file of files) {
      folder.write(file.name, file.content)
      const view = unwrap<{ id: string }>(
        'passwords_attachment_add',
        await instance.invoke('passwords_attachment_add', {
          args: { itemId, file: folder.path(file.name) },
        }),
      )
      ids.push(view.id)
    }
    return ids
  } finally {
    folder.remove()
  }
}

/** The folder of an entry, or the parent of a folder, as the backend holds it (`null` is the top). */
export async function placeOf(
  instance: Pick<FlowInstance, 'invoke'>,
  id: string,
): Promise<string | null | undefined> {
  const data = await overview(instance)
  const header = data.headers.find((candidate) => candidate.id === id)
  if (header) return header.groupId
  return data.groups.find((group) => group.id === id)?.parentId
}

/** Opens the right-click menu of the element found by hook (a `data-testid` or a selector,
 * `toSelector`), as a right click does (the rig has no pointer actions): a `contextmenu` event at
 * its centre. */
export async function contextMenu(
  instance: FlowInstance,
  hook: string,
  deadlineMs?: number,
): Promise<void> {
  await instance.waitForDisplayed(hook, deadlineMs)
  await instance.exec(
    `const el = document.querySelector(arguments[0])
     const box = el.getBoundingClientRect()
     el.dispatchEvent(new MouseEvent('contextmenu', {
       bubbles: true, cancelable: true, button: 2,
       clientX: box.left + Math.min(box.width / 2, 40), clientY: box.top + box.height / 2,
     }))
     return true`,
    [toSelector(hook)],
  )
}

/** Double-clicks the element found by hook, as the rig has no pointer actions: a `dblclick` event
 * (a file browser entry opens on it, a single click only selects). */
export async function doubleClick(
  instance: FlowInstance,
  hook: string,
  deadlineMs?: number,
): Promise<void> {
  await instance.waitForDisplayed(hook, deadlineMs)
  await instance.exec(
    `document.querySelector(arguments[0]).dispatchEvent(new MouseEvent('dblclick', {
       bubbles: true, cancelable: true, detail: 2,
     }))
     return true`,
    [toSelector(hook)],
  )
}

/** Drags the element found by `from` onto the one found by `to` with the events of HTML drag and
 * drop and one shared `DataTransfer`, as the browser does (the rig has no pointer actions). The
 * target is looked for once the drag has started: some only show while something is dragged (the
 * breadcrumbs of the password list during a selection). */
export async function dragTo(
  instance: FlowInstance,
  from: string,
  to: string,
): Promise<void> {
  await instance.waitForDisplayed(from)
  await instance.exec(
    `const source = document.querySelector('[data-testid="' + arguments[0] + '"]')
     const data = new DataTransfer()
     window.__e2eDrag = { source, data }
     source.dispatchEvent(new DragEvent('dragstart', { bubbles: true, cancelable: true, dataTransfer: data }))
     return true`,
    [from],
  )
  await instance.waitForDisplayed(to)
  await instance.exec(
    `const { source, data } = window.__e2eDrag
     delete window.__e2eDrag
     const target = document.querySelector('[data-testid="' + arguments[0] + '"]')
     const fire = (el, type) => el.dispatchEvent(new DragEvent(type, { bubbles: true, cancelable: true, dataTransfer: data }))
     fire(target, 'dragenter')
     fire(target, 'dragover')
     fire(target, 'drop')
     fire(source, 'dragend')
     return true`,
    [to],
  )
}

/** The ids of the rows the list marks as selected. */
export async function selectedRows(instance: FlowInstance): Promise<string[]> {
  return instance.exec<string[]>(
    `return [...document.querySelectorAll('[data-row-id][aria-pressed="true"]')].map((el) => el.dataset.rowId)`,
  )
}

/** Whether the list dims the row (a cut entry or folder in the Ablage). */
export async function rowDimmed(
  instance: FlowInstance,
  id: string,
): Promise<boolean> {
  return instance.exec<boolean>(
    `const row = document.querySelector('[data-row-id="' + arguments[0] + '"]')
     return Boolean(row && row.closest('li').classList.contains('opacity-50'))`,
    [id],
  )
}

/** The titles of the entries the device holds, sorted. */
export async function entryTitles(
  instance: Pick<FlowInstance, 'invoke'>,
): Promise<string[]> {
  return (await overview(instance)).headers
    .map((header) => header.title ?? '')
    .sort()
}

/** Waits until the device holds exactly these entry titles (fixed deadline, as the sync waits). */
export async function expectEntries(
  ctx: WaitContext,
  instance: Pick<FlowInstance, 'invoke'>,
  titles: string[],
  description: string,
): Promise<void> {
  const wanted = [...titles].sort()
  await ctx.waitFor(
    description,
    async () => {
      const have = await entryTitles(instance)
      return JSON.stringify(have) === JSON.stringify(wanted) ? have : false
    },
    { timeoutMs: 40_000, fixed: true },
  )
}

/** How many tags the device holds whose name equals `name` ignoring case. */
export async function tagCount(
  instance: Pick<FlowInstance, 'invoke'>,
  name: string,
): Promise<number> {
  const wanted = name.toLowerCase()
  return (await overview(instance)).tags.filter(
    (tag) => tag.name.toLowerCase() === wanted,
  ).length
}

/** Moves an entry to the trash (`passwords_trash`) or, when it is in it already, deletes it for good. */
export async function trashEntry(
  instance: Pick<FlowInstance, 'invoke'>,
  id: string,
): Promise<void> {
  unwrap(
    'passwords_trash',
    await instance.invoke('passwords_trash', {
      args: { targets: [{ kind: 'item', id }] },
    }),
  )
}

/** Takes an entry out of the trash. */
export async function restoreEntry(
  instance: Pick<FlowInstance, 'invoke'>,
  id: string,
): Promise<void> {
  unwrap(
    'passwords_restore',
    await instance.invoke('passwords_restore', {
      args: { targets: [{ kind: 'item', id }] },
    }),
  )
}

/** The tab of an open entry (spec 036) as the tab bar marks it, or `null` when no tab bar shows. */
export async function activeTab(
  instance: FlowInstance,
): Promise<string | null> {
  return instance.exec<string | null>(
    `const on = document.querySelector('[data-testid^="entry-tab-"][data-state="active"]')
     return on ? on.getAttribute('data-testid').slice('entry-tab-'.length) : null`,
  )
}

/**
 * Taps a tab of the open entry, scrolled into the middle first (in a long editor the tab bar can
 * sit under the top edge after typing further down), and waits until its slide stands still in
 * place: the tab bar switches at once, the slide moves for 300 ms, and a tap on a button of the
 * slide while it moves misses it.
 */
export async function selectTab(
  instance: FlowInstance,
  tab: 'details' | 'extra' | 'history',
): Promise<void> {
  await instance.waitForDisplayed(`entry-tab-${tab}`)
  await instance.exec(
    `document.querySelector('[data-testid="entry-tab-' + arguments[0] + '"]').scrollIntoView({ block: 'center' })
     return true`,
    [tab],
  )
  await instance.click(`entry-tab-${tab}`)
  const deadlineMs = 5000
  const end = Date.now() + deadlineMs
  for (;;) {
    const last = await slideUnsettled(instance, tab)
    if (last === null) return
    if (Date.now() >= end) {
      throw new Error(
        `timed out after ${deadlineMs} ms waiting for the ${tab} slide to stand still (last seen: ${last})`,
      )
    }
    await new Promise((resolve) => setTimeout(resolve, 50))
  }
}

/**
 * `null` when `tab`'s slide is the shown one, the swipe surface runs no transition and the slide
 * sits at the surface's left edge; otherwise what is still off. It reads what is on screen, not
 * Swiper's `animating`: a slide sent back before its transition got its first frame (an arrow key,
 * then a tap at once) makes WebKit cancel the transition without a `transitionend`, and Swiper,
 * which clears `animating` only on that event, keeps it set although nothing moves.
 */
async function slideUnsettled(
  instance: FlowInstance,
  tab: 'details' | 'extra' | 'history',
): Promise<string | null> {
  return instance.exec<string | null>(
    `const surface = document.querySelector('[data-testid="entry-tabs-swiper"]')
     const slide = document.querySelector('[data-testid="entry-panel-' + arguments[0] + '"]')
     if (!surface || !slide) return 'no swipe surface or slide'
     if (!slide.classList.contains('swiper-slide-active')) return 'another slide is shown'
     const wrapper = slide.parentElement
     const moving = typeof wrapper.getAnimations === 'function'
       ? wrapper.getAnimations().map((animation) => animation.transitionProperty || 'an animation')
       : []
     if (moving.length > 0) return 'the slides move (' + moving.join(', ') + ')'
     const off = slide.getBoundingClientRect().left - surface.getBoundingClientRect().left
     return Math.abs(off) > 1 ? 'the slide sits ' + Math.round(off) + ' px off' : null`,
    [tab],
  )
}

/** Whether this tab's slide has come to rest: active, and its left edge on the swipe surface's.
 * A click during the slide animation lets WebDriver scroll the surface sideways to reach the
 * moving target, which leaves the slides offset. */
export async function slideSettled(
  instance: FlowInstance,
  tab: 'details' | 'extra' | 'history',
): Promise<boolean> {
  return instance.exec<boolean>(
    `const slide = document.querySelector('[data-testid="entry-panel-' + arguments[0] + '"]')
     const surface = slide && slide.closest('.swiper')
     if (!slide || !surface || !slide.classList.contains('swiper-slide-active')) return false
     return Math.abs(slide.getBoundingClientRect().left - surface.getBoundingClientRect().left) < 1`,
    [tab],
  )
}

/** Whether the swipe surface shows this tab's slide: the slide follows the tab bar (spec 036 FR-002). */
export async function slideShows(
  instance: FlowInstance,
  tab: 'details' | 'extra' | 'history',
): Promise<boolean> {
  return instance.exec<boolean>(
    `const slide = document.querySelector('[data-testid="entry-panel-' + arguments[0] + '"]')
     return Boolean(slide && slide.classList.contains('swiper-slide-active'))`,
    [tab],
  )
}

/** Changes an entry through `passwords_update_item` (reading its update token first), so a scenario
 * can make states for the history without the editor. */
export async function updateEntry(
  instance: Pick<FlowInstance, 'invoke'>,
  id: string,
  patch: Record<string, unknown>,
): Promise<void> {
  const item = unwrap<{ updatedAt: string }>(
    'passwords_get_item',
    await instance.invoke('passwords_get_item', { args: { itemId: id } }),
  )
  unwrap(
    'passwords_update_item',
    await instance.invoke('passwords_update_item', {
      args: { itemId: id, expectedUpdatedAt: item.updatedAt, patch },
    }),
  )
}

/** The ids of the states of an entry, newest first, as the backend lists them. */
export async function historyIds(
  instance: Pick<FlowInstance, 'invoke'>,
  id: string,
): Promise<string[]> {
  return unwrap<Array<{ id: string }>>(
    'passwords_history_list',
    await instance.invoke('passwords_history_list', { args: { itemId: id } }),
  ).map((state) => state.id)
}

/** How far anything in the password manager frame sticks out to the right or scrolls sideways, in
 * pixels (FR-042: nothing at 360 px). Content clipped by an ancestor does not count. */
export async function horizontalOverflow(
  instance: FlowInstance,
): Promise<number> {
  return instance.exec<number>(
    `const frame = document.querySelector('[data-passwords-app]').closest('[data-wm-window-id]')
     const right = frame.getBoundingClientRect().right
     const clipped = (el) => {
       for (let node = el.parentElement; node && node !== frame; node = node.parentElement) {
         if (getComputedStyle(node).overflowX !== 'visible') return true
       }
       return false
     }
     let worst = 0
     for (const el of frame.querySelectorAll('*')) {
       const style = getComputedStyle(el)
       if (style.visibility === 'hidden') continue
       if (style.overflowX === 'auto' || style.overflowX === 'scroll') {
         worst = Math.max(worst, el.scrollWidth - el.clientWidth)
       }
       if (!clipped(el)) worst = Math.max(worst, el.getBoundingClientRect().right - right)
     }
     return worst`,
  )
}
