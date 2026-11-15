// What the page looked like when a scenario failed, as text: a screenshot shows pixels, but not the
// address, the focused element, the dialogs that were open or the test ids that were there. A wait
// that ends with "timed out (last observed: false)" is explained by this file, without a second run
// (the operator's rule: every test carries enough diagnostics to see the cause from the failure).

/** What `PAGE_STATE_SCRIPT` reads from the page. */
export interface PageState {
  href: string
  title: string
  readyState: string
  viewport: string
  focused: string
  dialogs: string[]
  toasts: string[]
  testIds: string[]
  text: string
}

/** The script a page runs (the body of a function) to describe itself. */
export const PAGE_STATE_SCRIPT = `
  const clip = (text, limit) => {
    const flat = String(text ?? '').replace(/\\s+/g, ' ').trim()
    return flat.length > limit ? flat.slice(0, limit) + '…' : flat
  }
  const shown = (el) => {
    const rect = el.getBoundingClientRect()
    const style = getComputedStyle(el)
    return rect.width > 0 && rect.height > 0 && style.visibility !== 'hidden' && style.display !== 'none'
  }
  const name = (el) => {
    if (!el) return 'nothing'
    const id = el.getAttribute('data-testid')
    return el.tagName.toLowerCase() + (el.id ? '#' + el.id : '') + (id ? '[data-testid=' + id + ']' : '')
  }
  const texts = (selector) =>
    [...document.querySelectorAll(selector)].filter(shown).map((el) => name(el) + ': ' + clip(el.innerText, 300))
  return {
    href: location.href,
    title: document.title,
    readyState: document.readyState,
    viewport: innerWidth + 'x' + innerHeight,
    focused: name(document.activeElement),
    dialogs: texts('[role="dialog"], [role="alertdialog"], dialog[open]'),
    toasts: texts('[data-sonner-toast], [role="status"], [role="alert"]'),
    testIds: [...document.querySelectorAll('[data-testid]')]
      .filter(shown)
      .map((el) => el.getAttribute('data-testid'))
      .slice(0, 150),
    text: clip(document.body ? document.body.innerText : '', 1500),
  }
`

const list = (items: string[]): string =>
  items.length === 0 ? '  none' : items.map((item) => `  - ${item}`).join('\n')

/** The page state as the lines of `page-state.txt`. */
export function formatPageState(state: PageState): string {
  return [
    `address:    ${state.href}`,
    `title:      ${state.title}`,
    `readyState: ${state.readyState}`,
    `viewport:   ${state.viewport}`,
    `focused:    ${state.focused}`,
    `dialogs open:`,
    list(state.dialogs),
    `toasts shown:`,
    list(state.toasts),
    `visible test ids (${state.testIds.length}, at most 150):`,
    state.testIds.length === 0 ? '  none' : `  ${state.testIds.join(', ')}`,
    `visible text:`,
    `  ${state.text === '' ? 'none' : state.text}`,
    '',
  ].join('\n')
}
