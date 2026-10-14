/**
 * Credentials as a form holds them and as Rust takes them (spec 038, spec 050
 * contracts/tauri-commands.md): an access key for S3, a password for WebDAV, and for SFTP a
 * password, a pasted key or a key holzi generated (only its id reaches the window, research R4).
 * Or an entry of the password manager the user chose (spec 052): only its id travels, its values
 * stay in Rust. The settings form and holzi's window for extensions share this.
 */
import type { CredentialsInput } from '../../../types/bindings/CredentialsInput.ts'
import type { Protocol } from '../../../types/bindings/Protocol.ts'

export type CredentialsDraft = {
  /** Typed in the form, or an entry of the password manager. */
  source: 'new' | 'entry'
  /** The chosen entry. */
  entryId: string
  accessKeyId: string
  secretAccessKey: string
  sessionToken: string
  username: string
  password: string
  authMethod: 'password' | 'key'
  privateKey: string
  passphrase: string
  /** A key holzi generated; its private half never reaches the window. */
  keyId: string
  publicKey: string
}

export function emptyCredentials(): CredentialsDraft {
  return {
    source: 'new',
    entryId: '',
    accessKeyId: '',
    secretAccessKey: '',
    sessionToken: '',
    username: '',
    password: '',
    authMethod: 'password',
    privateKey: '',
    passphrase: '',
    keyId: '',
    publicKey: '',
  }
}

/** Forgets the secrets of `draft` once they were sent. */
export function clearSecrets(draft: CredentialsDraft) {
  Object.assign(draft, {
    secretAccessKey: '',
    sessionToken: '',
    password: '',
    privateKey: '',
    passphrase: '',
    keyId: '',
    publicKey: '',
  })
}

/** The credentials of `draft` for a connection of `protocol`. */
export function credentialsInput(
  protocol: Protocol,
  draft: CredentialsDraft,
): CredentialsInput {
  if (draft.source === 'entry') return { kind: 'entry', itemId: draft.entryId }
  const username = draft.username.trim()
  if (protocol === 'sftp' && draft.authMethod === 'key')
    return draft.keyId
      ? { kind: 'generatedKey', username, keyId: draft.keyId }
      : {
          kind: 'sshKey',
          username,
          privateKey: draft.privateKey,
          passphrase: draft.passphrase || undefined,
        }
  if (protocol !== 's3')
    return { kind: 'password', username, password: draft.password }
  return {
    kind: 's3',
    accessKeyId: draft.accessKeyId.trim(),
    secretAccessKey: draft.secretAccessKey,
    sessionToken: draft.sessionToken.trim() || undefined,
  }
}

/** Whether `draft` has what a connection of `protocol` needs. */
export function credentialsComplete(
  protocol: Protocol,
  draft: CredentialsDraft,
): boolean {
  if (draft.source === 'entry') return !!draft.entryId
  if (protocol === 's3')
    return !!draft.accessKeyId.trim() && !!draft.secretAccessKey
  if (!draft.username.trim()) return false
  if (protocol === 'sftp' && draft.authMethod === 'key')
    return !!draft.keyId || !!draft.privateKey.trim()
  return !!draft.password
}
