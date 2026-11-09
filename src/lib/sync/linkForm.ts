// What a link text does to the link form of the landing page (spec 085, FR-009, FR-010). The
// text itself is read by the backend (`link_text_parse`); this only says which fields it fills.
// Pure, so `scripts/check-link-form.ts` tests it.

// The shape of the generated `LinkText` binding as far as the form uses it, declared here so the
// check script loads this file without the bindings.
type LinkTextShape = {
  code: string
  vaultName?: string
  servers?: { nostrRelays: string[]; irohRelays: string[]; disabled: string[] }
  droppedServers?: number
}

export type LinkFormFields = {
  code: string
  vaultName: string
  /** The Nostr servers added, the iroh servers added, and the servers switched off. */
  nostrRelays: string[]
  irohRelays: string[]
  disabledRelays: string[]
  /** The servers came with a link text, not from the user's own input. */
  serversFromLink: boolean
}

const LINK_SCHEME = 'haex-vault://'
/** Characters of a code without groups: 128 bits in Base32. */
const CODE_CHARS = 26

/** Whether `input` is more than a typed code, so the backend should read it. */
export function isLinkTextInput(input: string): boolean {
  const text = input.trim()
  if (text.toLowerCase().startsWith(LINK_SCHEME)) return true
  return text.replace(/[\s-]/g, '').length > CODE_CHARS
}

/**
 * The form after `parsed` was read: the code always, the vault name only into an empty field, the
 * servers when the text named any. A text without servers (a plain code) leaves the user's own
 * server input alone, but drops servers an earlier link text brought. Device name and passphrases
 * are not part of the fields and so are never touched.
 */
export function withLinkText(
  form: LinkFormFields,
  parsed: LinkTextShape,
): LinkFormFields {
  const vaultName =
    form.vaultName.trim() === '' && parsed.vaultName
      ? parsed.vaultName
      : form.vaultName
  if (parsed.servers) {
    return {
      code: parsed.code,
      vaultName,
      nostrRelays: [...parsed.servers.nostrRelays],
      irohRelays: [...parsed.servers.irohRelays],
      disabledRelays: [...parsed.servers.disabled],
      serversFromLink: true,
    }
  }
  if (form.serversFromLink) {
    return {
      code: parsed.code,
      vaultName,
      nostrRelays: [],
      irohRelays: [],
      disabledRelays: [],
      serversFromLink: false,
    }
  }
  return { ...form, code: parsed.code, vaultName }
}

/** The entries a link text brought, for the one line that replaces the server blocks. */
export function serversTakenOver(form: LinkFormFields): number {
  return (
    form.nostrRelays.length +
    form.irohRelays.length +
    form.disabledRelays.length
  )
}
