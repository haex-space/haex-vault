/**
 * TOTP/HOTP parsing utilities for the password importers.
 *
 * Extracted from the bitwarden / lastpass / keepass importers, which each
 * shipped a near-identical `parseOtpData` differing only in a whitespace-strip
 * flag and KeePass's extra recovery paths (separate `TOTP Seed`/`TOTP Settings`
 * fields, otpauth URI embedded in notes). Centralising the parsing keeps the
 * field layout interpretation in one place and makes it testable.
 */

export interface ParsedOtp {
  /** Base32 OTP secret, upper-cased. */
  secret: string
  /** Number of digits (typically 6 or 8). */
  digits: number
  /** Time-step / period in seconds (typically 30). */
  period: number
  /** HMAC algorithm name (`SHA1`, `SHA256`, `SHA512`), upper-cased. */
  algorithm: string
}

const DEFAULT_OTP: Omit<ParsedOtp, 'secret'> = {
  digits: 6,
  period: 30,
  algorithm: 'SHA1',
}

export interface ParseOtpOptions {
  /**
   * When the input is a bare secret (not an otpauth:// URI), strip ASCII
   * whitespace from it before upper-casing. LastPass occasionally exports
   * grouped secrets like `JBSW Y3DP EHPK 3PXP` — turning that into a usable
   * Base32 string needs whitespace removal. Bitwarden never does this so it
   * keeps the secret untouched.
   */
  stripWhitespace?: boolean
}

/**
 * Parse a TOTP source string. Accepts either:
 *   - an `otpauth://` URI (parameters `secret`, `digits`, `period`, `algorithm`)
 *   - a bare Base32 secret (defaults applied: 6 digits, 30s period, SHA1)
 *
 * Returns `null` for empty/missing input or for an otpauth URI without a
 * `secret` parameter. Malformed URIs are treated as "no OTP" rather than an
 * error — the importer continues with the rest of the entry.
 */
export function parseOtpData(
  value: string | null | undefined,
  options: ParseOtpOptions = {},
): ParsedOtp | null {
  if (!value?.trim()) return null

  if (value.startsWith('otpauth://')) {
    return parseOtpUri(value)
  }

  const secret = options.stripWhitespace
    ? value.toUpperCase().replace(/\s/g, '')
    : value.toUpperCase()

  return { secret, ...DEFAULT_OTP }
}

/**
 * Parse an `otpauth://` URI. Exported separately so importers (notably
 * KeePass) can recover an OTP that an exporter stuffed into the notes field
 * rather than into a dedicated OTP slot.
 */
export function parseOtpUri(uri: string): ParsedOtp | null {
  try {
    const url = new URL(uri)
    const secret = url.searchParams.get('secret')
    if (!secret) return null
    return {
      secret: secret.toUpperCase(),
      digits: parseInt(url.searchParams.get('digits') ?? '6', 10),
      period: parseInt(url.searchParams.get('period') ?? '30', 10),
      algorithm: (url.searchParams.get('algorithm') ?? 'SHA1').toUpperCase(),
    }
  }
  catch {
    return null
  }
}
