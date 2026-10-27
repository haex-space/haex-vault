// Helpers for scenarios that share a contact (spec 060): building the text of a share through the
// command, and the `holzi:card:` text of the card inside it, which the Visitenkarte import of spec
// 051 still reads until the import of shares replaces it.
import { unwrap } from './flows.ts'
import type { FlowInstance } from './flows.ts'

/** The id of the own contact, the same in every vault (`SELF_CONTACT_ID` in Rust). */
export const SELF_CONTACT_ID = '1ce52853-b579-4ffd-b9ef-f27071a414ed'

const LINK_PREFIX = 'haex-vault://contact/'

function fromBase64Url(text: string): Uint8Array {
  return Uint8Array.from(
    Buffer.from(text.replace(/-/g, '+').replace(/_/g, '/'), 'base64'),
  )
}

function toBase64Url(bytes: Uint8Array): string {
  return Buffer.from(bytes)
    .toString('base64')
    .replace(/\+/g, '-')
    .replace(/\//g, '_')
    .replace(/=+$/, '')
}

/** A reader of postcard: bytes, LEB128 lengths, `Option` as a tag byte. */
class Reader {
  private at = 0
  private readonly bytes: Uint8Array

  constructor(bytes: Uint8Array) {
    this.bytes = bytes
  }

  byte(): number {
    const value = this.bytes[this.at]
    if (value === undefined) throw new Error('the share ends early')
    this.at += 1
    return value
  }

  length(): number {
    let value = 0
    let shift = 0
    for (;;) {
      const next = this.byte()
      value += (next & 0x7f) * 2 ** shift
      if ((next & 0x80) === 0) return value
      shift += 7
    }
  }

  take(count: number): Uint8Array {
    if (this.at + count > this.bytes.length)
      throw new Error('the share ends early')
    const slice = this.bytes.slice(this.at, this.at + count)
    this.at += count
    return slice
  }

  text(): string {
    return new TextDecoder().decode(this.take(this.length()))
  }

  optionalText(): void {
    if (this.byte() === 1) this.text()
  }
}

/** What a share carries, as far as scenarios read it. */
export interface ReadShare {
  name: string
  values: Array<{ kind: string; label: string; value: string }>
  card: Uint8Array | null
}

/** Reads the text of a share (`ShareEnvelope` in Rust, postcard inside base64url). */
export function readShare(link: string): ReadShare {
  if (!link.startsWith(LINK_PREFIX)) throw new Error(`not a share: ${link}`)
  const reader = new Reader(fromBase64Url(link.slice(LINK_PREFIX.length)))
  reader.byte() // version
  const name = reader.text()
  for (let part = 0; part < 5; part += 1) reader.optionalText() // name parts
  for (let field = 0; field < 5; field += 1) reader.optionalText() // head
  const values: ReadShare['values'] = []
  const count = reader.length()
  for (let value = 0; value < count; value += 1) {
    values.push({
      kind: reader.text(),
      label: reader.text(),
      value: reader.text(),
    })
  }
  const card = reader.byte() === 1 ? reader.take(reader.length()) : null
  return { name, values, card }
}

/** The card bytes inside the text of a share, or `null` for a share without a card. */
export function cardBytesOf(link: string): Uint8Array | null {
  return readShare(link).card
}

/** The `holzi:card:` text of the card inside a share, which the older import reads. */
export function cardTextOf(link: string): string {
  const card = cardBytesOf(link)
  if (!card) throw new Error('the share carries no card')
  return `holzi:card:${toBase64Url(card)}`
}

/** The text of a share of the contact with everything the dialog offers chosen. */
export async function shareLink(
  instance: Pick<FlowInstance, 'invoke'>,
  contactId: string,
): Promise<string> {
  const options = unwrap<{
    rows: Array<{ id: string }>
    head: Record<string, string | null | undefined>
    holzi:
      | { kind: 'none' }
      | { kind: 'card' }
      | {
          kind: 'own'
          devices: Array<{ devicePubkey: string }>
          relays: string[]
        }
  }>(
    'contacts_share_options',
    await instance.invoke('contacts_share_options', { args: { contactId } }),
  )
  const filled = (field: string) => Boolean(options.head[field])
  const selection = {
    values: options.rows.map((row) => row.id),
    displayName: filled('displayName'),
    nickname: filled('nickname'),
    organization: filled('organization'),
    jobTitle: filled('jobTitle'),
    note: filled('note'),
    holzi:
      options.holzi.kind === 'own'
        ? {
            kind: 'own',
            devices: options.holzi.devices.map((device) => device.devicePubkey),
            relays: options.holzi.relays,
          }
        : { kind: options.holzi.kind },
  }
  return unwrap<{ link: string }>(
    'contacts_share_build',
    await instance.invoke('contacts_share_build', {
      args: { contactId, selection },
    }),
  ).link
}

/** The Visitenkarte of the vault behind `instance`, named `name`, as the older import reads it. */
export async function ownCardText(
  instance: Pick<FlowInstance, 'invoke'>,
  name: string,
): Promise<string> {
  unwrap(
    'contacts_update',
    await instance.invoke('contacts_update', {
      args: { id: SELF_CONTACT_ID, patch: { givenName: name } },
    }),
  )
  return cardTextOf(await shareLink(instance, SELF_CONTACT_ID))
}
