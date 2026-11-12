// The marker of an agent tab (spec 065, FR-014, FR-017, research R6). holzi adds it to every
// document of the tab's main frame from its own isolated world: a bar in a closed shadow root with
// the agent's name, "Eingreifen" (or "Zurückgeben" while the user has the tab) and "Stoppen", and a
// prefix on the tab's title. The page shares the DOM but not this world: it cannot reach the bar's
// buttons, cannot call the binding `__holzi`, and its synthetic events are not trusted. It can
// remove the bar; the bar comes back a few times, and holzi's own window stays the binding place
// to stop (FR-015).
//
// A trusted press of a mouse button or a key anywhere in the page, outside the bar, reports
// `input`: holzi pauses the agent when the input is not its own (FR-017). Each new bar says
// `hello`, and holzi sets the state the session has now; run again with a new config (the agent
// paused or steering again), the script updates the bar that is there.
// Rust wraps the declaration in parentheses and calls it, so nothing here calls it.
// eslint-disable-next-line no-unused-vars
function holziMarker(config) {
  // A key that only changes a modifier (Alt, Super, Ctrl, Shift) is not the user working in the
  // page: Alt+Tab or the Super key to leave the browser reaches the focused page as a trusted key
  // press first, and must not take the browser from the agent.
  const MODIFIER_KEYS = [
    'Alt',
    'AltGraph',
    'CapsLock',
    'Control',
    'Meta',
    'OS',
    'Shift',
    'Super',
  ]
  // The click that brings the browser to the front reaches the page as a press right after the window
  // got the focus: it focuses the window, it is not the user working in the page (it is
  // FOCUS_CLICK_MS long). A second press, or any key, counts.
  const FOCUS_CLICK_MS = 400
  // Installed once per document, though the marker runs again with each new state. Only the
  // window's own focus counts: an element of the page that gets the focus (a field clicked or
  // reached with Tab) must not make the next press look like a focus click.
  if (globalThis.__holziFocusedAt === undefined) {
    // No focus yet: a press in the first moments of a document is the user's, not a focus click.
    globalThis.__holziFocusedAt = -Infinity
    window.addEventListener('focus', (event) => {
      if (event.target === window)
        globalThis.__holziFocusedAt = performance.now()
    })
  }
  // Whether a trusted press or key is the user working in the page. The first press within
  // FOCUS_CLICK_MS of the window's focus is the click that focused it and is spent here: the next
  // press, a double click for one, counts.
  const byUser = (event) => {
    if (!event.isTrusted) return false
    if (event.type === 'keydown') return !MODIFIER_KEYS.includes(event.key)
    if (performance.now() - globalThis.__holziFocusedAt < FOCUS_CLICK_MS) {
      globalThis.__holziFocusedAt = -Infinity
      return false
    }
    return true
  }
  if (window !== window.top) {
    // Input in an embedded frame does not bubble to the main frame. Install only the input
    // listener here: the visible marker and title belong to the main frame.
    if (!globalThis.__holziInputListener) {
      const noticed = (event) => {
        if (byUser(event)) globalThis.__holzi('input')
      }
      window.addEventListener('pointerdown', noticed, true)
      window.addEventListener('keydown', noticed, true)
      globalThis.__holziInputListener = noticed
    }
    return
  }
  if (globalThis.__holziMarker) return globalThis.__holziMarker.update(config)
  const MAX_RESTORES = 20
  let restores = 0
  let current = config

  const styled = (element, styles) => {
    // Style properties, not a style attribute or sheet: a page's CSP cannot refuse them, and
    // `important` beats the page's own rules for the host.
    for (const [name, value] of Object.entries(styles))
      element.style.setProperty(name, value, 'important')
    return element
  }

  const host = styled(document.createElement('holzi-agent-marker'), {
    all: 'initial',
    position: 'fixed',
    top: '8px',
    left: '50%',
    transform: 'translateX(-50%)',
    'z-index': '2147483647',
    display: 'block',
  })
  const root = host.attachShadow({ mode: 'closed' })
  const bar = styled(document.createElement('div'), {
    display: 'flex',
    'align-items': 'center',
    gap: '10px',
    padding: '6px 8px 6px 12px',
    'border-radius': '999px',
    background: '#1f2937',
    color: '#f9fafb',
    font: '13px/1.4 system-ui, sans-serif',
    'box-shadow': '0 2px 10px rgba(0, 0, 0, 0.35)',
    'max-width': 'min(90vw, 560px)',
  })
  const label = styled(document.createElement('span'), {
    overflow: 'hidden',
    'text-overflow': 'ellipsis',
    'white-space': 'nowrap',
  })
  const button = (background) => {
    const made = styled(document.createElement('button'), {
      all: 'initial',
      cursor: 'pointer',
      padding: '3px 12px',
      'border-radius': '999px',
      background,
      color: '#ffffff',
      font: '600 13px/1.4 system-ui, sans-serif',
      'white-space': 'nowrap',
    })
    made.type = 'button'
    return made
  }
  const steer = button('#2563eb')
  const stop = button('#dc2626')
  const pressed = (what) => (event) => {
    if (!event.isTrusted) return
    event.stopPropagation()
    globalThis.__holzi(what())
  }
  steer.addEventListener(
    'click',
    pressed(() => (current.paused ? 'return' : 'takeover')),
  )
  stop.addEventListener(
    'click',
    pressed(() => 'stop'),
  )
  bar.append(label, steer, stop)
  root.append(bar)

  const render = () => {
    label.textContent = current.paused ? current.pausedText : current.text
    steer.textContent = current.paused ? current.giveBack : current.takeOver
    stop.textContent = current.stop
  }
  render()

  // The user's own input in the page while the agent steers. holzi tells its own input apart.
  const noticed = (event) => {
    if (!byUser(event) || current.paused) return
    if (event.composedPath().includes(host)) return
    globalThis.__holzi('input')
  }
  window.addEventListener('pointerdown', noticed, true)
  window.addEventListener('keydown', noticed, true)

  // A page that goes fullscreen would hide this bar and its "Stoppen" (FR-014, FR-055): leave it
  // again at once and say so. While the user has the tab, fullscreen is theirs to allow.
  const leaveFullscreen = () => {
    if (!document.fullscreenElement || current.paused) return
    document.exitFullscreen().catch(() => {})
    globalThis.__holzi('fullscreen')
  }
  document.addEventListener('fullscreenchange', leaveFullscreen)
  // Fullscreen that began before this bar was there, or while the user had the tab.
  leaveFullscreen()

  const prefixed = () => {
    const title = document.title
    if (!title.startsWith(current.prefix))
      document.title = current.prefix + title
  }

  const place = () => {
    const parent = document.documentElement
    if (parent && !host.isConnected) parent.appendChild(host)
  }

  // From the document's start: the bar goes in as soon as there is an element to hold it, and
  // comes back when the page removes it.
  let placed = false
  const watching = new MutationObserver(() => {
    if (!host.isConnected && document.documentElement) {
      if (!placed) placed = true
      else if (restores++ >= MAX_RESTORES) return prefixed()
      place()
    }
    prefixed()
  })
  watching.observe(document, {
    childList: true,
    subtree: true,
    characterData: true,
  })

  globalThis.__holziMarker = {
    get prefix() {
      return current.prefix
    },
    update(config) {
      current = config
      render()
      leaveFullscreen()
    },
    remove() {
      watching.disconnect()
      window.removeEventListener('pointerdown', noticed, true)
      window.removeEventListener('keydown', noticed, true)
      document.removeEventListener('fullscreenchange', leaveFullscreen)
      host.remove()
      if (document.title.startsWith(current.prefix))
        document.title = document.title.slice(current.prefix.length)
      delete globalThis.__holziMarker
    },
  }

  if (document.documentElement) {
    placed = true
    place()
    prefixed()
  }
  globalThis.__holzi('hello')
}
