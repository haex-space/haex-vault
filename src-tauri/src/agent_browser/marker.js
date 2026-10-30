// The marker of an agent tab (spec 065, FR-014, research R6). holzi adds it to every document of
// the tab's main frame from its own isolated world: a bar in a closed shadow root with the agent's
// name and "Stoppen", and a prefix on the tab's title. The page shares the DOM but not this world:
// it cannot reach the bar's buttons, cannot call the binding `__holzi`, and its synthetic events
// are not trusted. It can remove the bar; the bar comes back a few times, and holzi's own window
// stays the binding place to stop (FR-015).
// Rust wraps the declaration in parentheses and calls it, so nothing here calls it.
// eslint-disable-next-line no-unused-vars
function holziMarker(config) {
  if (window !== window.top || globalThis.__holziMarker) return
  const MAX_RESTORES = 20
  let restores = 0

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
  label.textContent = config.text
  const stop = styled(document.createElement('button'), {
    all: 'initial',
    cursor: 'pointer',
    padding: '3px 12px',
    'border-radius': '999px',
    background: '#dc2626',
    color: '#ffffff',
    font: '600 13px/1.4 system-ui, sans-serif',
  })
  stop.type = 'button'
  stop.textContent = config.stop
  stop.addEventListener('click', (event) => {
    if (!event.isTrusted) return
    event.stopPropagation()
    globalThis.__holzi('stop')
  })
  bar.append(label, stop)
  root.append(bar)

  const prefixed = () => {
    const title = document.title
    if (!title.startsWith(config.prefix)) document.title = config.prefix + title
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
    prefix: config.prefix,
    remove() {
      watching.disconnect()
      host.remove()
      if (document.title.startsWith(config.prefix))
        document.title = document.title.slice(config.prefix.length)
      delete globalThis.__holziMarker
    },
  }

  if (document.documentElement) {
    placed = true
    place()
    prefixed()
  }
}
