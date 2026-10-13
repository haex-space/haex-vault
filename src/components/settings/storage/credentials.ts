/**
 * Credentials as a form holds them and as Rust takes them (spec 038, spec 050
 * contracts/tauri-commands.md): an access key for S3, a password for WebDAV, and for SFTP a
 * password, a pasted key or a key holzi generated (only its id reaches the window, research R4).
 * The settings form and holzi's window for extensions share this.
 */
import type { CredentialsInput } from '@bindings/CredentialsInput'
import type { Protocol } from '@bindings/Protocol'

export type CredentialsDraft = {
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

/** The credentials of `draft` for a connection of `protocol`. */
export function credentialsInput(
  protocol: Protocol,
  draft: CredentialsDraft,
): CredentialsInput {
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
  if (protocol === 's3')
    return !!draft.accessKeyId.trim() && !!draft.secretAccessKey
  if (!draft.username.trim()) return false
  if (protocol === 'sftp' && draft.authMethod === 'key')
    return !!draft.keyId || !!draft.privateKey.trim()
  return !!draft.password
}
