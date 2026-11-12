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

  // The fields holzi filled from the password manager in this document (research R7); this world
  // keeps them, the page cannot see or change the set.
  const filledFields = (globalThis.holziFilled ||= new WeakSet())
  const isSecret = (el) =>
    (el.tagName.toLowerCase() === 'input' && el.type === 'password') ||
    filledFields.has(el)

  // What a form would send, for the submit question: name, value, whether it is secret, and the
  // kind it declares.
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
      .map((field) => [
        field.name,
        String(field.value || ''),
        isSecret(field),
        // What the field declares about its kind, for "does the form send personal data".
        `${field.type || ''} ${field.getAttribute('autocomplete') || ''}`
          .toLowerCase()
          .trim(),
      ])
  }
  const submitterOf = (el) => {
    const form = el.form || el.closest('form')
    if (!form) return ''
    const submitter = Array.from(formElements.call(form)).find(
      (field) =>
        !field.disabled &&
        ((field.tagName.toLowerCase() === 'button' &&
          ['', 'submit'].includes(
            (field.getAttribute('type') || '').toLowerCase(),
          )) ||
          (field.tagName.toLowerCase() === 'input' &&
            ['submit', 'image'].includes((field.type || '').toLowerCase()))),
    )
    return submitter ? nameOf(submitter) : ''
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
  if (args.hrefOf !== undefined && args.hrefOf !== null) {
    const el = operable[args.hrefOf - 1]
    if (!el || !el.href) return { gone: true }
    return { href: String(el.href), url: location.href }
  }
  if (args.scrollTo !== undefined && args.scrollTo !== null) {
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

  // Where an element is and what it is, for the question before an action on it.
  const targetInfo = (el) => {
    const r = el.getBoundingClientRect()
    return {
      x: r.left + r.width / 2,
      y: r.top + r.height / 2,
      kind: kindOf(el),
      name: cut(nameOf(el), MAX_NAME),
      facts: factsOf(el),
      fields: fieldsOf(el),
      defaultSubmit: submitterOf(el),
      formAutocomplete:
        (el.form || el.closest('form'))?.getAttribute('autocomplete') || '',
      url: location.href,
    }
  }

  if (args.target !== undefined && args.target !== null) {
    const el = numbered(args.target)
    if (!el) return { gone: true }
    return targetInfo(el)
  }
  // Gives element `focusElement` the keyboard focus without selecting its text and without moving
  // the page more than the focus does; the same facts as a target.
  if (args.focusElement !== undefined && args.focusElement !== null) {
    const el = numbered(args.focusElement)
    if (!el) return { gone: true }
    el.focus()
    return targetInfo(el)
  }
  // The element that has the keyboard focus, following open shadow roots down; the same facts as
  // a target, without moving the page. A key press is checked as the element it goes to.
  if (args.active) {
    let el = document.activeElement
    while (el && el.shadowRoot && el.shadowRoot.activeElement)
      el = el.shadowRoot.activeElement
    if (!el || el === document.body || el === document.documentElement)
      return { none: true, url: location.href }
    return targetInfo(el)
  }
  // The point a wheel turns over (the middle of element `probe.element`, else of the view) and
  // where the document and the nearest scroller under that point are now, so the caller can tell
  // whether the wheel moved anything.
  if (args.probe) {
    let x = innerWidth / 2
    let y = innerHeight / 2
    if (args.probe.element !== undefined && args.probe.element !== null) {
      const el = numbered(args.probe.element)
      if (!el) return { gone: true }
      const r = el.getBoundingClientRect()
      x = Math.min(Math.max(r.left + r.width / 2, 0), innerWidth - 1)
      y = Math.min(Math.max(r.top + r.height / 2, 0), innerHeight - 1)
    } else if (args.probe.x !== undefined && args.probe.x !== null) {
      x = args.probe.x
      y = args.probe.y
    }
    let scroller = document.elementFromPoint(x, y)
    while (scroller && scroller.shadowRoot) {
      const inner = scroller.shadowRoot.elementFromPoint(x, y)
      if (!inner || inner === scroller) break
      scroller = inner
    }
    while (scroller && scroller !== document.documentElement) {
      const style = getComputedStyle(scroller)
      const scrolls = (overflow, size, client) =>
        ['auto', 'scroll'].includes(overflow) && size > client
      if (
        scrolls(
          style.overflowY,
          scroller.scrollHeight,
          scroller.clientHeight,
        ) ||
        scrolls(style.overflowX, scroller.scrollWidth, scroller.clientWidth)
      )
        break
      scroller = scroller.parentElement
    }
    const inner =
      scroller && scroller !== document.documentElement
        ? [scroller.scrollLeft, scroller.scrollTop]
        : []
    return {
      x,
      y,
      size: [innerWidth, innerHeight],
      pos: [Math.round(scrollX), Math.round(scrollY), ...inner.map(Math.round)],
    }
  }
  if (args.focus !== undefined && args.focus !== null) {
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
  if (args.formOf !== undefined && args.formOf !== null) {
    const el = operable[args.formOf - 1]
    if (!el) return { gone: true }
    return {
      fields: fieldsOf(el),
      form: factsOf(el).form,
      defaultSubmit: submitterOf(el),
      formAutocomplete:
        (el.form || el.closest('form'))?.getAttribute('autocomplete') || '',
    }
  }
  // The file field of an element: the field itself, the field its label belongs to, one inside
  // it, or the only one of its form (or page), since a styled button often opens a hidden field.
  const fileFieldOf = (el) => {
    const isFile = (node) =>
      node &&
      node.tagName &&
      node.tagName.toLowerCase() === 'input' &&
      (node.getAttribute('type') || '').toLowerCase() === 'file'
    if (isFile(el)) return el
    const label = el.closest && el.closest('label')
    if (label && isFile(label.control)) return label.control
    const inner = el.querySelector && el.querySelector('input[type="file" i]')
    if (inner) return inner
    const scope = el.form || (el.closest && el.closest('form')) || document
    const all = scope.querySelectorAll('input[type="file" i]')
    return all.length === 1 ? all[0] : null
  }
  if (args.fileOf !== undefined && args.fileOf !== null) {
    const el = numbered(args.fileOf)
    if (!el) return { gone: true }
    const field = fileFieldOf(el)
    if (!field) return { notFile: true }
    return {
      name: cut(nameOf(el) || nameOf(field), MAX_NAME),
      field: cut(field.getAttribute('name') || '', MAX_NAME),
      url: location.href,
    }
  }
  if (args.fileElement !== undefined && args.fileElement !== null) {
    const el = operable[args.fileElement - 1]
    return el ? fileFieldOf(el) : null
  }
  // The fields a fill may write: the inputs of a form that take text, in document order. `form`
  // is the number of an element of the form; without one, the form with a password field, else the
  // page.
  const NOT_TEXT = [
    'hidden',
    'submit',
    'button',
    'reset',
    'image',
    'file',
    'checkbox',
    'radio',
  ]
  const fillable = (form) => {
    let scope
    if (form !== undefined && form !== null) {
      const el = operable[form - 1]
      if (!el) return null
      scope = el.form || (el.closest && el.closest('form')) || document
    } else {
      const password = document.querySelector('input[type="password" i]')
      scope =
        (password && (password.form || password.closest('form'))) || document
    }
    return Array.from(scope.querySelectorAll('input'))
      .filter(
        (el) =>
          !NOT_TEXT.includes((el.getAttribute('type') || '').toLowerCase()),
      )
      .filter((el) => rendered(el) && !el.disabled && !el.readOnly)
      .slice(0, 50)
  }
  if (args.fillFields !== undefined) {
    const fields = fillable(args.fillFields)
    if (!fields) return { gone: true }
    return {
      url: location.href,
      fields: fields.map((el) => ({
        inputType: (el.getAttribute('type') || 'text').toLowerCase(),
        name: el.getAttribute('name') || '',
        id: el.id || '',
        autocomplete: el.getAttribute('autocomplete') || '',
        placeholder: el.getAttribute('placeholder') || '',
      })),
      // For the user's question only (entry fill); never part of the mapping.
      labels: fields.map((el) => cut(nameOf(el), MAX_NAME)),
    }
  }
  if (args.fill !== undefined && args.fill !== null) {
    if (location.href !== args.fill.url)
      return { changed: true, url: location.href }
    const fields = fillable(args.fill.form)
    if (!fields) return { gone: true }
    const factsOfFillable = (el) => ({
      inputType: (el.getAttribute('type') || 'text').toLowerCase(),
      name: el.getAttribute('name') || '',
      id: el.id || '',
      autocomplete: el.getAttribute('autocomplete') || '',
      placeholder: el.getAttribute('placeholder') || '',
    })
    const currentFacts = fields.map(factsOfFillable)
    const expectedFacts = args.fill.fields
    if (
      currentFacts.length !== expectedFacts.length ||
      currentFacts.some((field, index) =>
        Object.keys(field).some(
          (key) => field[key] !== expectedFacts[index][key],
        ),
      )
    ) {
      return { changed: true, url: location.href }
    }
    const setValue = Object.getOwnPropertyDescriptor(
      HTMLInputElement.prototype,
      'value',
    ).set
    const filled = []
    for (const [index, value] of args.fill.values) {
      const el = fields[index]
      if (!el) continue
      el.focus()
      setValue.call(el, value)
      el.dispatchEvent(new Event('input', { bubbles: true }))
      el.dispatchEvent(new Event('change', { bubbles: true }))
      filledFields.add(el)
      const n = operable.indexOf(el)
      if (n >= 0) filled.push(n + 1)
    }
    return { filled, url: location.href }
  }
  // A new password (browser.credentials.create, research R7): the password field of element
  // `element` and the next password field of its form, the repeat field, if it is empty or holzi's.
  // Without a value it only reports; an empty value empties both again.
  if (args.newPassword !== undefined && args.newPassword !== null) {
    if (
      args.newPassword.url !== undefined &&
      args.newPassword.url !== null &&
      location.href !== args.newPassword.url
    )
      return { changed: true, url: location.href }
    const el = operable[args.newPassword.element - 1]
    if (!el) return { gone: true }
    if (el.tagName.toLowerCase() !== 'input' || el.type !== 'password')
      return { notPassword: true, url: location.href }
    const value = args.newPassword.value
    if (value === undefined || value === null) return { url: location.href }
    const scope = el.form || el.closest('form')
    const repeat = scope
      ? Array.from(scope.querySelectorAll('input[type="password" i]')).find(
          (other) =>
            other !== el &&
            el.compareDocumentPosition(other) &
              Node.DOCUMENT_POSITION_FOLLOWING &&
            rendered(other) &&
            !other.disabled &&
            !other.readOnly &&
            (other.value === '' || filledFields.has(other)),
        )
      : null
    const setValue = Object.getOwnPropertyDescriptor(
      HTMLInputElement.prototype,
      'value',
    ).set
    const filled = []
    for (const field of repeat ? [el, repeat] : [el]) {
      field.focus()
      setValue.call(field, value)
      field.dispatchEvent(new Event('input', { bubbles: true }))
      field.dispatchEvent(new Event('change', { bubbles: true }))
      filledFields.add(field)
      const n = operable.indexOf(field)
      if (n >= 0) filled.push(n + 1)
    }
    return { filled, url: location.href }
  }
  if (args.choose !== undefined && args.choose !== null) {
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
  if (args.section !== undefined && args.section !== null) {
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
