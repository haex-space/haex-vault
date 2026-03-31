import { invoke } from '@tauri-apps/api/core'

export interface SealedVaultName {
  encryptedData: string
  nonce: string
  salt: string
  ephemeralPublicKey: string
}

/**
 * Encrypt a vault name using the identity's Ed25519 public key.
 *
 * Rust internally converts Ed25519 → X25519, then performs:
 * ECDH (ephemeral X25519) → HKDF-SHA256 (with random salt) → AES-256-GCM
 */
export async function encryptVaultNameAsync(vaultName: string, identityPublicKey: string): Promise<SealedVaultName> {
  const plaintextB64 = btoa(vaultName)
  return invoke<SealedVaultName>('encrypt_for_identity', {
    plaintextB64,
    identityPublicKeyB64: identityPublicKey,
  })
}

/**
 * Decrypt a vault name using the identity's Ed25519 private key.
 *
 * Rust internally converts Ed25519 → X25519, then reverses ECDH + HKDF + AES-GCM.
 */
export async function decryptVaultNameAsync(
  encryptedData: string,
  nonce: string,
  salt: string,
  ephemeralPublicKey: string,
  identityPrivateKey: string,
): Promise<string> {
  const plaintextB64 = await invoke<string>('decrypt_for_identity', {
    encryptedDataB64: encryptedData,
    nonceB64: nonce,
    saltB64: salt,
    ephemeralPublicKeyB64: ephemeralPublicKey,
    identityPrivateKeyB64: identityPrivateKey,
  })
  return atob(plaintextB64)
}
