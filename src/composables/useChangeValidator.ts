interface IncomingChange {
  spaceId: string
  signedBy?: string | null
  signature?: string | null
  recordOwner?: string | null
  collaborative?: boolean | null
}

/**
 * Validate an incoming sync_change before applying it.
 * Phase 4: UCAN capability checks.
 * Phase 5: MLS membership + decrypt checks.
 */
export async function validateIncomingChange(change: IncomingChange): Promise<{ valid: boolean; error?: string }> {
  // Personal vault data (no signedBy) is always valid
  if (!change.signedBy) {
    return { valid: true }
  }

  // For shared space data: verify the signer has a valid UCAN
  // In Phase 4 we trust the server's validation (Ebene 1) and do basic checks
  // Full MLS membership verification comes in Phase 5

  // Basic check: signedBy must be present if it's a shared space change
  if (!change.signature) {
    return { valid: false, error: 'Missing signature for shared space change' }
  }

  // Record ownership check
  // If recordOwner is set and doesn't match signedBy, check collaborative flag
  if (change.recordOwner && change.recordOwner !== change.signedBy && !change.collaborative) {
    return { valid: false, error: 'Cannot modify non-collaborative record owned by another user' }
  }

  return { valid: true }
}
