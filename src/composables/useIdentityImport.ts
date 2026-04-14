import { didKeyToPublicKeyAsync } from '@haex-space/vault-sdk'

export interface ParsedIdentityImport {
  name: string
  publicKey?: string
  did?: string
  privateKey?: string
  avatar?: string | null
  claims: Array<{ type: string; value: string }>
}

export interface ImportOptions {
  selectedClaimIndices: Set<number>
  includeAvatar: boolean
}

export type ImportResultKind = 'identity' | 'contact'

export interface ImportResult {
  kind: ImportResultKind
}

/**
 * Thrown when the pasted/selected JSON cannot be parsed.
 * Callers should surface a user-visible "invalid JSON" message.
 */
export class InvalidImportJsonError extends Error {
  constructor() {
    super('Could not parse import JSON')
    this.name = 'InvalidImportJsonError'
  }
}

/**
 * Thrown when the parsed JSON does not contain the minimum required fields
 * (at least `did`). A missing private-key just downgrades it from
 * identity to contact.
 */
export class InvalidImportDataError extends Error {
  constructor() {
    super('Parsed data is missing required identity fields')
    this.name = 'InvalidImportDataError'
  }
}

/**
 * Encapsulates the two-step identity/contact import flow:
 *   - `parseImport(rawJson)`: shape-check + normalise into `ParsedIdentityImport`
 *   - `importAsync(parsed, options)`: dispatches to `importIdentityAsync` when
 *     a private key is present, else to `addContactWithClaimsAsync`.
 *
 * UI concerns (file dialog, toasts, stepper) live in the consumer.
 */
export function useIdentityImport() {
  const identityStore = useIdentityStore()

  const parseImport = (rawJson: string): ParsedIdentityImport => {
    let parsed: Record<string, unknown>
    try {
      parsed = JSON.parse(rawJson)
    } catch {
      throw new InvalidImportJsonError()
    }

    const did = typeof parsed.did === 'string' ? parsed.did : undefined
    const publicKey =
      typeof parsed.publicKey === 'string' ? parsed.publicKey : undefined

    if (!did && !publicKey) {
      throw new InvalidImportDataError()
    }

    const claims = Array.isArray(parsed.claims)
      ? (parsed.claims as Array<{ type: string; value: string }>)
      : []

    return {
      name: (parsed.name as string) || '',
      publicKey,
      did,
      privateKey: parsed.privateKey as string | undefined,
      avatar: typeof parsed.avatar === 'string' ? parsed.avatar : null,
      claims,
    }
  }

  const importAsync = async (
    data: ParsedIdentityImport,
    options: ImportOptions,
  ): Promise<ImportResult> => {
    const selectedClaims = data.claims.filter((_, i) =>
      options.selectedClaimIndices.has(i),
    )
    const avatar = options.includeAvatar ? data.avatar : null

    if (data.privateKey && data.did) {
      await identityStore.importIdentityAsync({
        did: data.did,
        name: data.name,
        publicKey: data.publicKey,
        privateKey: data.privateKey,
        avatar,
        claims: selectedClaims,
      })
      return { kind: 'identity' }
    }

    const contact = await identityStore.addContactWithClaimsAsync(
      data.name ||
        `Imported ${(data.did || data.publicKey || '').slice(0, 16)}...`,
      data.publicKey || (await didKeyToPublicKeyAsync(data.did!)),
      selectedClaims,
    )
    if (avatar) {
      await identityStore.updateContactAsync(contact.id, { avatar })
    }
    return { kind: 'contact' }
  }

  return { parseImport, importAsync }
}
