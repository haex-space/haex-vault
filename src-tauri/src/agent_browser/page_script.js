// The page script of the agent browser (spec 065, research R4, R10, R12). It runs in holzi's own
// isolated world, so the page can neither call nor replace it; it sees the same DOM. It returns
// the page's title, address, readable text (the main content first) and the operable elements,
// numbered in document order. Password fields never report a value. Rust cuts the result
// (read.rs); this script only bounds what it hands over.
// Rust wraps the declaration in parentheses and calls it, so nothing here calls it.
// eslint-disable-next-line no-unused-vars
function holziPage(args) {
  const MAX_TEXT = 100000
  const MAX_ELEMENTS = 2000
  const MAX_NAME = 100
  const OPERABLE = [
    'a[href]',
    'button',
    'input:not([type="hidden"])',
    'select',
    'textarea',
    'summary',
    '[contenteditable="true"]',
    '[role="button"]',
    '[role="link"]',
    '[role="checkbox"]',
    '[role="radio"]',
    '[role="tab"]',
    '[role="menuitem"]',
    '[role="switch"]',
    '[role="combobox"]',
  ].join(',')

  const clean = (text) => (text || '').replace(/\s+/g, ' ').trim()
  const cut = (text, max) =>
    text.length > max ? text.slice(0, max - 1) + '…' : text

  const rendered = (el) => {
    if (!el.getClientRects().length) return false
    const style = getComputedStyle(el)
    return style.visibility !== 'hidden' && style.display !== 'none'
  }

  const inView = (el) => {
    const r = el.getBoundingClientRect()
    return (
      r.bottom > 0 && r.right > 0 && r.top < innerHeight && r.left < innerWidth
    )
  }

  const kindOf = (el) => {
    const tag = el.tagName.toLowerCase()
    const role = el.getAttribute('role')
    const type = (el.getAttribute('type') || '').toLowerCase()
    if (tag === 'a' || role === 'link') return 'link'
    if (tag === 'select' || role === 'combobox') return 'select'
    if (type === 'checkbox' || role === 'checkbox' || role === 'switch')
      return 'checkbox'
    if (type === 'radio' || role === 'radio') return 'radio'
    if (
      tag === 'input' &&
      ['submit', 'button', 'reset', 'image'].includes(type)
    )
      return 'button'
    if (tag === 'input' || tag === 'textarea' || el.isContentEditable)
      return 'field'
    if (role === 'tab') return 'tab'
    if (role === 'menuitem') return 'menuitem'
    return 'button'
  }

  const nameOf = (el) => {
    const label = el.getAttribute('aria-label')
    if (clean(label)) return clean(label)
    const by = el.getAttribute('aria-labelledby')
    if (by) {
      const text = by
        .split(/\s+/)
        .map((id) => document.getElementById(id))
        .filter(Boolean)
        .map((node) => node.textContent)
        .join(' ')
      if (clean(text)) return clean(text)
    }
    if (el.labels && el.labels.length) {
      const text = Array.from(el.labels)
        .map((node) => node.textContent)
        .join(' ')
      if (clean(text)) return clean(text)
    }
    for (const attribute of ['placeholder', 'title', 'alt']) {
      const value = el.getAttribute(attribute)
      if (clean(value)) return clean(value)
    }
    const tag = el.tagName.toLowerCase()
    if (tag === 'input' && ['submit', 'button', 'reset'].includes(el.type))
      return clean(el.value)
    const image = el.querySelector && el.querySelector('img[alt]')
    return (
      clean(el.innerText) || (image ? clean(image.getAttribute('alt')) : '')
    )
  }

  const describe = (el, n) => {
    const kind = kindOf(el)
    const item = {
      n,
      kind,
      name: cut(nameOf(el), MAX_NAME),
      visible: inView(el),
    }
    const secret =
      el.tagName.toLowerCase() === 'input' && el.type === 'password'
    if (secret) item.secret = true
    else if (kind === 'field')
      item.value = cut(
        el.isContentEditable ? clean(el.innerText) : el.value || '',
        500,
      )
    else if (kind === 'select' && el.tagName.toLowerCase() === 'select') {
      item.value = el.selectedOptions.length
        ? clean(el.selectedOptions[0].textContent)
        : ''
      item.options = Array.from(el.options)
        .slice(0, 50)
        .map((option) => cut(clean(option.textContent), MAX_NAME))
    }
    if (kind === 'checkbox' || kind === 'radio')
      item.checked =
        el.checked === true || el.getAttribute('aria-checked') === 'true'
    if (el.disabled || el.getAttribute('aria-disabled') === 'true')
      item.disabled = true
    if (el.required || el.getAttribute('aria-required') === 'true')
      item.required = true
    return item
  }

  const operable = Array.from(document.querySelectorAll(OPERABLE))
    .filter(rendered)
    .slice(0, MAX_ELEMENTS)

  if (args.scrollBy) {
    scrollBy(0, args.scrollBy * Math.round(innerHeight * 0.8))
    return { scrolled: true }
  }
  if (args.hrefOf) {
    const el = operable[args.hrefOf - 1]
    if (!el || !el.href) return { gone: true }
    return { href: String(el.href) }
  }
  if (args.scrollTo) {
    const el = operable[args.scrollTo - 1]
    if (!el) return { gone: true }
    el.scrollIntoView({ block: 'center' })
    return { scrolled: true }
  }

  let root = null
  if (args.section) {
    const el = operable[args.section - 1]
    if (!el) return { gone: true }
    root =
      el.closest(
        'section, article, form, fieldset, li, tr, table, nav, aside, main',
      ) ||
      el.parentElement ||
      el
  }

  let text
  if (root) {
    text = root.innerText || ''
  } else {
    const body = document.body ? document.body.innerText || '' : ''
    const main = document.querySelector('main, [role="main"], article')
    const mainText = main && main !== document.body ? main.innerText || '' : ''
    text = mainText ? mainText + '\n\n' + body.replace(mainText, '') : body
  }
  text = text
    .replace(/\n{3,}/g, '\n\n')
    .trim()
    .slice(0, MAX_TEXT)

  const scope = root ? (el) => root.contains(el) : () => true
  const elements = []
  operable.forEach((el, index) => {
    if (scope(el)) elements.push(describe(el, index + 1))
  })

  // The marker's prefix on the title is holzi's, not the page's.
  const marker = globalThis.__holziMarker
  let title = document.title || ''
  if (marker && title.startsWith(marker.prefix))
    title = title.slice(marker.prefix.length)

  return { title, url: location.href, text, elements }
}
