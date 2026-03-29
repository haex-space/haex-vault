import { invoke } from '@tauri-apps/api/core'

export interface SealedVaultName {
  encryptedData: string
  nonce: string
  ephemeralPublicKey: string
}

/**
 * Encrypt a vault name with the identity's X25519 agreement public key.
 * Uses Rust for X25519 ECDH (WebCrypto X25519 not available in all WebViews).
 */
export async function encryptVaultNameAsync(vaultName: string, agreementPublicKey: string): Promise<SealedVaultName> {
  const plaintextB64 = btoa(vaultName)
  return invoke<SealedVaultName>('x25519_encrypt', {
    plaintextB64,
    recipientPublicKeyB64: agreementPublicKey,
  })
}

/**
 * Decrypt a vault name with the identity's X25519 agreement private key.
 */
export async function decryptVaultNameAsync(
  encryptedData: string,
  nonce: string,
  ephemeralPublicKey: string,
  agreementPrivateKey: string,
): Promise<string> {
  const plaintextB64 = await invoke<string>('x25519_decrypt', {
    encryptedDataB64: encryptedData,
    nonceB64: nonce,
    ephemeralPublicKeyB64: ephemeralPublicKey,
    privateKeyB64: agreementPrivateKey,
  })
  return atob(plaintextB64)
}
