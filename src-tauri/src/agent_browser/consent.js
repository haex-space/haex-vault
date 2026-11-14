// The consent script of the agent browser (spec 065 FR-065, research R23): it declines the optional
// cookies of a consent banner, and only that. It runs in holzi's own world in every document of an
// agent's tab, frames included, from the start of the document, so a banner that appears late is
// found too. It never accepts: it looks for a button that rejects (by the known platforms' own
// buttons, else by the words on a button inside something that talks about cookies), and where the
// banner only offers "settings" it opens them once and looks again. Choices that must be made one
// by one (a "reject" button for each purpose, or a switch for each) are all declined and then
// saved; the one save button that is pressed is the one of a panel whose every optional choice
// was declined. A panel that will not save until the user consents (a consent wall: agree, or
// subscribe) is left as it is. What it found is kept in `globalThis.__holziConsent.state` for the
// reading of the page (`searching`, `none`, `declined`, `open`, `wall`) and told to holzi
// through the binding `__holzi`.
// Rust passes nothing in; the script runs as it is.
;(() => {
  if (globalThis.__holziConsent) return
  // `stopped` is set by holzi when the tab is given back: nothing is pressed after that.
  const consent = (globalThis.__holziConsent = {
    state: 'searching',
    stopped: false,
  })

  // How long a document is watched, and how long after the start a page without a banner counts as
  // having none (the reading of the page waits for that, so it is short).
  const WATCH_MS = 15000
  const NONE_MS = 900
  // How long a banner without a way to reject stays before the agent is told it stays.
  const GIVE_UP_MS = 1200
  // How long a save button may stay unavailable, after everything that can be declined was, before
  // the panel counts as a consent wall.
  const WALL_MS = 1500
  // The most elements one look goes through, and the most choices one document is made to decline.
  const MAX_SCAN = 4000
  const MAX_PRESS = 80
  const started = performance.now()

  // The reject buttons of the consent platforms, by their own ids and classes. Only identifiers
  // that belong to a platform are listed: a button by its title or words could be on any page, so
  // those are only taken inside a banner (see `look`).
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
  // What saves the choices of a panel, once every optional one was declined.
  const SAVE_PHRASES = [
    'einstellungen anwenden',
    'einstellungen speichern',
    'auswahl speichern',
    'auswahl bestätigen',
    'auswahl übernehmen',
    'einstellungen übernehmen',
    'speichern',
    'bestätigen',
    'übernehmen',
    'save settings',
    'save choices',
    'save preferences',
    'save my choices',
    'save and exit',
    'save & exit',
    'confirm my choices',
    'confirm choices',
    'confirm selection',
    'apply settings',
    'save',
    'confirm',
    'enregistrer',
    'confirmer mes choix',
    'valider mes choix',
    'sauvegarder',
    'guardar',
    'confirmar mis opciones',
    'confirmar selección',
    'salva',
    'salva le preferenze',
    'conferma le mie scelte',
    'opslaan',
    'keuze bevestigen',
    'salvar',
    'confirmar escolhas',
  ]
  // A button that says any of these accepts; it is never a save button.
  const ACCEPTS =
    /akzeptier|zustimm|einwillig|erlaub|alle auswählen|accept|agree|allow|select all|accepter|aceptar|accetta|accepteren|aceitar/i
  // A choice that cannot be declined: it is what the page needs to work.
  const NEEDED =
    /notwendig|erforderlich|essenzi|essentiell|technisch|required|necessary|essential|strictly|nécessaire|necesari|necessari|noodzakelijk|obrigat/i
  // "Non-essential", "nicht notwendige": the same words, but for the choices that are optional.
  // Whatever one of those names is taken out before NEEDED looks at a label.
  const NOT_NEEDED = new RegExp(
    `(?:\\bnon|\\bnot|\\bun|\\bno|\\bnicht|\\bkeine?|\\bniet|\\bgeen|\\bnão|\\bpas|\\bsin)[-\\s]*(?:${NEEDED.source})\\w*`,
    'gi',
  )
  const needed = (label) => NEEDED.test(label.replace(NOT_NEEDED, ' '))
  // Something that talks about cookies; the id or class of something that may be a banner. Words
  // that only say "privacy" or "tracking" are not enough: a dialog about those (a permission, a
  // policy that changed) may have a "decline" button that is none of ours to press.
  const ABOUT_COOKIES =
    /cookie|ciasteczk|consent|einwilligung|toestemming|gdpr|dsgvo|rodo/i
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

  // A button whose words reject everything at once.
  const rejectsAll = (say) =>
    say.length > 0 &&
    (REJECT_PHRASES.some((phrase) => say.includes(phrase)) ||
      (say.length <= 30 &&
        REJECT_WORDS.some(
          (word) => say === `${word} cookies` || say === `${word} all`,
        )))

  // A button that is only the word: alone it rejects, one for each purpose it declines one.
  const rejectsOne = (say) =>
    say.length > 0 && REJECT_WORDS.some((word) => say === word)

  // Some pages word the save button as agreeing, but to the selection that was made, never to
  // everything ("agree to the selected").
  const SELECTION =
    /auswahl|ausgewählt|selected|selection|selecci|sélection|selezion|geselecteerd|seleção/i
  const EVERYTHING = /\balle[ns]?\b|\ball\b|\btout|\btod[oa]s?\b|\btutt[oi]\b/i
  const SAYS_YES =
    /zustimm|akzept|erlaub|bestätig|speicher|accept|agree|allow|confirm|save|accepter|aceptar|accetta/i

  const saves = (say) =>
    say.length > 0 &&
    say.length <= 40 &&
    ((!ACCEPTS.test(say) &&
      SAVE_PHRASES.some(
        (phrase) => say === phrase || say.startsWith(`${phrase} `),
      )) ||
      (SELECTION.test(say) && !EVERYTHING.test(say) && SAYS_YES.test(say)))

  const opensSettings = (say) =>
    say.length > 0 &&
    say.length <= 40 &&
    SETTINGS_WORDS.some((word) => say.includes(word))

  // A control that cannot be pressed now (a banner may enable its buttons a moment after it shows).
  const enabled = (control) =>
    !control.matches(':disabled') &&
    control.getAttribute('aria-disabled') !== 'true'

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

  // The choices of a panel that were pressed (a button counts by the pair it is in, and its place
  // in it, so a page that draws the pressed one again is not pressed twice, while the buttons of
  // several purposes that share one parent are told apart), and when the last one was.
  const pressed = new WeakMap()
  const placeOf = (control) => {
    const owner = control.tagName === 'BUTTON' ? control.parentElement : null
    return owner
      ? [owner, Array.prototype.indexOf.call(owner.children, control)]
      : [control, 0]
  }
  const wasPressed = (control) => {
    const [owner, slot] = placeOf(control)
    return pressed.get(owner)?.has(slot) === true
  }
  const markPressed = (control) => {
    const [owner, slot] = placeOf(control)
    const slots = pressed.get(owner) || new Set()
    slots.add(slot)
    pressed.set(owner, slots)
  }
  let pressedCount = 0
  let pressedAt = 0

  // What a switch or a box is for, as far as the words around it say.
  // A box has its own label (`for`, or the one around it) before the text of what it sits in,
  // which may name its neighbours too.
  const labelOf = (control) => {
    const own = control.labels
      ? Array.from(control.labels, (label) => label.innerText).join(' ')
      : ''
    const named = (control.getAttribute('aria-labelledby') || '')
      .split(/\s+/)
      .map((id) => control.getRootNode().getElementById?.(id)?.innerText || '')
      .join(' ')
    return (
      control.getAttribute('aria-label') ||
      own.trim() ||
      named.trim() ||
      control.closest('label')?.innerText ||
      control.parentElement?.innerText ||
      ''
    )
      .slice(0, 160)
      .toLowerCase()
  }

  // The switches of a container that are on and may be turned off: not the ones the page needs.
  const switchesOn = (container) =>
    Array.from(
      container.querySelectorAll(
        'input[type="checkbox"], [role="switch"], [role="checkbox"]',
      ),
    ).filter((control) => {
      const on =
        control.tagName === 'INPUT'
          ? control.checked
          : control.getAttribute('aria-checked') === 'true'
      return on && enabled(control) && !needed(labelOf(control))
    })

  // The button that rejects: a known platform's own, else one inside a banner that talks about
  // cookies. `settings` is the one that opens the choices, when there is no reject button. A
  // `panel` is a banner whose choices are made one by one: the buttons or switches that decline
  // them, and the button that saves.
  const look = () => {
    const surveyed = survey()
    for (const root of surveyed.roots) {
      for (const selector of KNOWN) {
        const el = Array.from(root.querySelectorAll(selector)).find(
          (candidate) => visible(candidate) && enabled(candidate),
        )
        if (el) return { reject: el }
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
      // A disabled button presses nothing: it is never the one that rejects or opens (the look is
      // repeated until it is enabled or the watch ends), but a disabled save button is what tells
      // a consent wall. A container only gets here when it talks about cookies, so its choices may
      // be made one by one.
      const usable = controls.filter(enabled)
      const all = usable.find((control) => rejectsAll(sayOf(control)))
      if (all) return { reject: all }
      const rows = usable.filter((control) => rejectsOne(sayOf(control)))
      const switches = switchesOn(container)
      const save = controls.find((control) => saves(sayOf(control)))
      // Once something was declined the panel stays one, though nothing is left to decline.
      if (save && (rows.length || switches.length || pressedCount > 0))
        return { panel: { rows, switches, save } }
      if (rows.length === 1) return { reject: rows[0] }
      if (rows.length > 1) return { panel: { rows, switches, save: null } }
      settings ||= usable.find((control) => opensSettings(sayOf(control)))
    }
    return { banner, settings }
  }

  const tell = (word) => {
    if (typeof globalThis.__holzi === 'function') globalThis.__holzi(word)
  }

  const done = (state, word) => {
    consent.state = state
    tell(word)
  }

  // Declines every choice there is to decline, then saves; a save button that stays unavailable
  // means the page does not take the choices without a consent that is not optional.
  const settle = (panel) => {
    let acted = false
    let left = false
    for (const control of [...panel.rows, ...panel.switches]) {
      if (wasPressed(control)) continue
      if (pressedCount >= MAX_PRESS) {
        // Not all of the choices can be declined: nothing is saved with some of them still on.
        left = true
        continue
      }
      markPressed(control)
      pressedCount += 1
      control.click()
      acted = true
    }
    const now = performance.now()
    if (!acted && left) {
      if (consent.state !== 'open') done('open', 'consent:open')
      return
    }
    if (acted || !pressedAt) {
      pressedAt = now
      // The page draws the new state before the save button is looked at.
      if (acted) return
    }
    if (!panel.save) {
      if (now - pressedAt > GIVE_UP_MS && consent.state !== 'open')
        done('open', 'consent:open')
      return
    }
    if (!enabled(panel.save)) {
      if (now - pressedAt > WALL_MS && consent.state !== 'wall')
        done('wall', 'consent:wall')
      return
    }
    panel.save.click()
    done('declined', 'consent:declined')
  }

  let openedSettings = false
  let bannerSince = 0
  const attempt = () => {
    if (consent.state === 'declined' || consent.stopped) return
    const found = look()
    if (found.reject) {
      found.reject.click()
      done('declined', 'consent:declined')
      return
    }
    if (found.panel) {
      bannerSince ||= performance.now()
      settle(found.panel)
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
        consent.state !== 'wall' &&
        performance.now() - bannerSince > GIVE_UP_MS
      ) {
        done('open', 'consent:open')
      }
    } else if (pressedAt && consent.state === 'searching') {
      // What was pressed took the banner away.
      done('declined', 'consent:declined')
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
    if (age > WATCH_MS || consent.state === 'declined' || consent.stopped)
      return
    try {
      if (document.body) attempt()
    } catch {
      // A page that breaks the look is left alone; the next look tries again.
    }
    setTimeout(schedule, age < 3000 ? 300 : 1000)
  }
  schedule()
})()
