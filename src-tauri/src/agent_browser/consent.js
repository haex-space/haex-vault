// The consent script of the agent browser (spec 065 FR-065, research R23): it declines the optional
// cookies of a consent banner, and only that. It runs in holzi's own world in every document of an
// agent's tab, frames included, from the start of the document, so a banner that appears late is
// found too. It never accepts: it looks for a button that rejects (by the known platforms' own
// buttons, else by the words on a button inside something that talks about cookies), and where the
// banner only offers "settings" it opens them once and looks again. What it found is kept in
// `globalThis.__holziConsent.state` for the reading of the page (`searching`, `none`, `declined`,
// `open`) and told to holzi through the binding `__holzi`.
// Rust passes nothing in; the script runs as it is.
;(() => {
  if (globalThis.__holziConsent) return
  const consent = (globalThis.__holziConsent = { state: 'searching' })

  // How long a document is watched, and how long after the start a page without a banner counts as
  // having none (the reading of the page waits for that, so it is short).
  const WATCH_MS = 15000
  const NONE_MS = 900
  // How long a banner without a way to reject stays before the agent is told it stays.
  const GIVE_UP_MS = 1200
  // The most elements one look goes through.
  const MAX_SCAN = 4000
  const started = performance.now()

  // The reject buttons of the consent platforms, by their own ids and classes.
  const KNOWN = [
    '#onetrust-reject-all-handler',
    '.ot-pc-refuse-all-handler',
    '#CybotCookiebotDialogBodyButtonDecline',
    '#didomi-notice-disagree-button',
    'button[data-testid="uc-deny-all-button"]',
    '#cookiescript_reject',
    '.cky-btn-reject',
    '.cmplz-deny',
    '.iubenda-cs-reject-btn',
    '.osano-cmp-deny',
    '#cn-refuse-cookie',
    '.cc-deny',
    '.cn-decline',
    '._brlbs-refuse-btn',
    '[data-cookiefirst-action="reject"]',
    '.qc-cmp2-summary-buttons button[mode="secondary"]',
    '#truste-consent-required',
    '.sp_choice_type_REJECT_ALL',
    'button[title="Reject all" i]',
    'button[title="Alle ablehnen" i]',
  ]

  // What a button says that rejects: phrases that say it by themselves, and single words that say
  // it when they are all the button says.
  const REJECT_PHRASES = [
    'alle ablehnen',
    'alles ablehnen',
    'alle cookies ablehnen',
    'nur notwendige',
    'nur erforderliche',
    'nur essenzielle',
    'nur essentielle',
    'nur technisch notwendige',
    'ohne zustimmung',
    'weiter ohne akzeptieren',
    'weiter ohne zustimmen',
    'nicht zustimmen',
    'alle verweigern',
    'reject all',
    'reject non-essential',
    'reject optional',
    'reject additional',
    'decline all',
    'decline optional',
    'deny all',
    'refuse all',
    'only necessary',
    'necessary only',
    'only essential',
    'essential only',
    'only required',
    'required only',
    'continue without accepting',
    'continue without agreeing',
    'do not accept',
    'i disagree',
    'tout refuser',
    'refuser tout',
    'continuer sans accepter',
    'uniquement nécessaires',
    'seulement nécessaires',
    'rechazar todo',
    'rechazar todas',
    'continuar sin aceptar',
    'solo necesarias',
    'solo esenciales',
    'rifiuta tutto',
    'rifiuta tutti',
    'solo necessari',
    'continua senza accettare',
    'alles weigeren',
    'alle weigeren',
    'alleen noodzakelijke',
    'alleen functionele',
    'doorgaan zonder accepteren',
    'rejeitar tudo',
    'rejeitar todos',
    'apenas necessários',
    'somente necessários',
    'continuar sem aceitar',
    'odrzuć wszystkie',
    'odrzuć wszystko',
    'tylko niezbędne',
    'avvisa alla',
    'avslå alle',
    'afvis alle',
  ]
  const REJECT_WORDS = [
    'ablehnen',
    'verweigern',
    'reject',
    'decline',
    'deny',
    'refuse',
    'disagree',
    'refuser',
    'rechazar',
    'rifiuta',
    'weigeren',
    'rejeitar',
    'odrzuć',
    'avvisa',
    'avslå',
    'afvis',
  ]
  // What opens the choices of a banner that has no button to reject at once.
  const SETTINGS_WORDS = [
    'einstellungen',
    'anpassen',
    'individuell',
    'weitere optionen',
    'mehr optionen',
    'settings',
    'manage',
    'preferences',
    'customize',
    'customise',
    'more options',
    'paramétrer',
    'personnaliser',
    'gérer',
    'préférences',
    'configurar',
    'personalizar',
    'gestionar',
    'ajustes',
    'impostazioni',
    'gestisci',
    'personalizza',
    'instellingen',
    'voorkeuren',
    'beheren',
    'definições',
    'configurações',
    'gerir',
  ]
  // Something that talks about cookies; the id or class of something that may be a banner.
  const ABOUT_COOKIES =
    /cookie|consent|einwilligung|datenschutz|privacy|privatsph|gdpr|dsgvo|tracking|zustimmung|vie privée|privacidad|privacy policy|toestemming/i
  const BANNER_NAME =
    /cookie|consent|gdpr|dsgvo|cmp|privacy|onetrust|didomi|usercentrics|borlabs|cky|truste|sp[-_]?message|cc[-_]?(window|banner)|klaro|osano|iubenda|termly|qc-cmp|banner|notice|popup|modal|overlay|dialog/i
  const CONTROLS =
    'button, [role="button"], a, input[type="button"], input[type="submit"]'

  const norm = (text) =>
    (text || '')
      .replace(/\s+/g, ' ')
      .trim()
      .toLowerCase()
      .replace(/^[^\p{L}]+|[^\p{L}]+$/gu, '')

  const visible = (el) => {
    if (!el.getClientRects().length) return false
    const style = getComputedStyle(el)
    return style.visibility !== 'hidden' && style.display !== 'none'
  }

  // Every element of the document and of its open shadow roots (a closed one cannot be reached),
  // and the roots themselves, as far as one look goes.
  const survey = () => {
    const elements = []
    const roots = [document]
    const stack = [document]
    while (stack.length && elements.length < MAX_SCAN) {
      const root = stack.pop()
      for (const el of root.querySelectorAll('*')) {
        if (elements.length >= MAX_SCAN) break
        elements.push(el)
        if (el.shadowRoot) {
          roots.push(el.shadowRoot)
          stack.push(el.shadowRoot)
        }
      }
    }
    return { elements, roots }
  }

  const sayOf = (control) =>
    norm(
      control.tagName === 'INPUT'
        ? control.value
        : control.innerText ||
            control.textContent ||
            control.getAttribute('aria-label') ||
            control.getAttribute('title'),
    )

  const rejects = (say) =>
    say.length > 0 &&
    (REJECT_PHRASES.some((phrase) => say.includes(phrase)) ||
      (say.length <= 30 &&
        REJECT_WORDS.some(
          (word) =>
            say === word || say === `${word} cookies` || say === `${word} all`,
        )))

  const opensSettings = (say) =>
    say.length > 0 &&
    say.length <= 40 &&
    SETTINGS_WORDS.some((word) => say.includes(word))

  // A link that leaves the page is not a button to press here.
  const pressable = (control) => {
    if (control.tagName !== 'A') return true
    const href = control.getAttribute('href')
    return !href || href === '#' || href.startsWith('javascript:')
  }

  // The things that may be a banner: dialogs, and what is named like one, and shadow roots.
  const containers = ({ elements, roots }) => {
    const found = []
    for (const el of elements) {
      const named = `${el.id || ''} ${el.getAttribute('class') || ''}`
      if (
        el.matches(
          '[role="dialog"], [role="alertdialog"], [aria-modal="true"]',
        ) ||
        BANNER_NAME.test(named)
      )
        found.push(el)
    }
    return found.concat(roots.slice(1))
  }

  const textOfContainer = (container) =>
    (container.innerText || container.textContent || '').slice(0, 3000)

  // The button that rejects: a known platform's own, else one inside a banner that talks about
  // cookies. `settings` is the one that opens the choices, when there is no reject button.
  const look = () => {
    const surveyed = survey()
    for (const root of surveyed.roots) {
      for (const selector of KNOWN) {
        const el = root.querySelector(selector)
        if (el && visible(el)) return { reject: el }
      }
    }
    let banner = false
    let settings = null
    for (const container of containers(surveyed)) {
      if (container.nodeType === 1 && !visible(container)) continue
      if (!ABOUT_COOKIES.test(textOfContainer(container))) continue
      const controls = Array.from(container.querySelectorAll(CONTROLS)).filter(
        (control) => visible(control) && pressable(control),
      )
      if (!controls.length) continue
      banner = true
      const reject = controls.find((control) => rejects(sayOf(control)))
      if (reject) return { reject }
      settings ||= controls.find((control) => opensSettings(sayOf(control)))
    }
    return { banner, settings }
  }

  const tell = (word) => {
    if (typeof globalThis.__holzi === 'function') globalThis.__holzi(word)
  }

  let openedSettings = false
  let bannerSince = 0
  const attempt = () => {
    if (consent.state === 'declined') return
    const found = look()
    if (found.reject) {
      found.reject.click()
      consent.state = 'declined'
      tell('consent:declined')
      return
    }
    if (found.banner) {
      bannerSince ||= performance.now()
      // Only "settings": open them once and look again at what shows.
      if (found.settings && !openedSettings) {
        openedSettings = true
        found.settings.click()
      }
      // A banner that stays without a way to reject is left, and the agent is told.
      if (
        consent.state !== 'open' &&
        performance.now() - bannerSince > GIVE_UP_MS
      ) {
        consent.state = 'open'
        tell('consent:open')
      }
    } else if (
      consent.state === 'searching' &&
      performance.now() - started > NONE_MS &&
      document.readyState === 'complete'
    ) {
      consent.state = 'none'
    }
  }

  // Looks often at first and then less often, until the document has been watched long enough.
  const schedule = () => {
    const age = performance.now() - started
    if (age > WATCH_MS || consent.state === 'declined') return
    try {
      if (document.body) attempt()
    } catch {
      // A page that breaks the look is left alone; the next look tries again.
    }
    setTimeout(schedule, age < 3000 ? 300 : 1000)
  }
  schedule()
})()
