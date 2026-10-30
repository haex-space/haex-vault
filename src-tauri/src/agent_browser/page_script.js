// The page script of the agent browser (spec 065, research R4, R10, R12). It runs in holzi's own
// isolated world, so the page can neither call nor replace it; it sees the same DOM. It returns
// the page's title, address, readable text (the main content first) and the operable elements,
// numbered in document order. Password fields never report a value. Rust cuts the result
// (read.rs) and decides from each element's facts whether a click submits (submit.rs); this
// script only bounds what it hands over. For clicking, typing and choosing it finds the element
// of a number, brings it into view and reports where it is (act.rs).
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

  const factsOf = (el) => ({
    tag: el.tagName.toLowerCase(),
    type: (el.getAttribute('type') || '').toLowerCase(),
    form: !!(el.form || el.closest('form')),
  })

  const isSecret = (el) =>
    el.tagName.toLowerCase() === 'input' && el.type === 'password'

  // What a form would send, for the submit question: name, value, and whether it is secret.
  const formElements = Object.getOwnPropertyDescriptor(
    HTMLFormElement.prototype,
    'elements',
  ).get
  const fieldsOf = (el) => {
    const form = el.form || el.closest('form')
    if (!form) return []
    return Array.from(formElements.call(form))
      .filter((field) => field.name && !field.disabled)
      .filter(
        (field) =>
          !['submit', 'button', 'reset', 'image', 'file'].includes(field.type),
      )
      .filter(
        (field) => !['checkbox', 'radio'].includes(field.type) || field.checked,
      )
      .slice(0, 30)
      .map((field) => [
        field.name,
        cut(String(field.value || ''), 200),
        isSecret(field),
      ])
  }

  const describe = (el, n) => {
    const kind = kindOf(el)
    const item = {
      n,
      kind,
      name: cut(nameOf(el), MAX_NAME),
      visible: inView(el),
    }
    const secret = isSecret(el)
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
    item.facts = factsOf(el)
    return item
  }

  const operable = Array.from(document.querySelectorAll(OPERABLE))
    .filter(rendered)
    .slice(0, MAX_ELEMENTS)

  if (args.scrollBy) {
    scrollBy(0, args.scrollBy * Math.round(innerHeight * 0.8))
    return { scrolled: true }
  }
  if (args.hrefOf !== undefined) {
    const el = operable[args.hrefOf - 1]
    if (!el || !el.href) return { gone: true }
    return { href: String(el.href), url: location.href }
  }
  if (args.scrollTo !== undefined) {
    const el = operable[args.scrollTo - 1]
    if (!el) return { gone: true }
    el.scrollIntoView({ block: 'center' })
    return { scrolled: true }
  }

  const numbered = (n) => {
    const el = operable[n - 1]
    if (el) el.scrollIntoView({ block: 'center', inline: 'center' })
    return el
  }

  if (args.target !== undefined) {
    const el = numbered(args.target)
    if (!el) return { gone: true }
    const r = el.getBoundingClientRect()
    return {
      x: r.left + r.width / 2,
      y: r.top + r.height / 2,
      kind: kindOf(el),
      name: cut(nameOf(el), MAX_NAME),
      facts: factsOf(el),
      fields: fieldsOf(el),
      url: location.href,
    }
  }
  if (args.focus !== undefined) {
    const el = numbered(args.focus)
    if (!el) return { gone: true }
    const kind = kindOf(el)
    if (kind !== 'field') return { kind, notField: true }
    el.focus()
    if (typeof el.select === 'function') el.select()
    else {
      const range = document.createRange()
      range.selectNodeContents(el)
      getSelection().removeAllRanges()
      getSelection().addRange(range)
    }
    return {
      kind,
      name: cut(nameOf(el), MAX_NAME),
      facts: factsOf(el),
      secret: isSecret(el),
      url: location.href,
    }
  }
  if (args.formOf !== undefined) {
    const el = operable[args.formOf - 1]
    if (!el) return { gone: true }
    return { fields: fieldsOf(el), form: factsOf(el).form }
  }
  if (args.choose !== undefined) {
    const el = numbered(args.choose)
    if (!el) return { gone: true }
    if (el.tagName.toLowerCase() !== 'select') return { notSelect: true }
    const wanted = clean(String(args.value)).toLowerCase()
    const option = Array.from(el.options).find(
      (o) =>
        o.value === args.value || clean(o.textContent).toLowerCase() === wanted,
    )
    if (!option) return { noOption: true }
    el.value = option.value
    el.dispatchEvent(new Event('input', { bubbles: true }))
    el.dispatchEvent(new Event('change', { bubbles: true }))
    return {
      chosen: clean(option.textContent),
      name: cut(nameOf(el), MAX_NAME),
      url: location.href,
    }
  }

  let root = null
  if (args.section !== undefined) {
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
