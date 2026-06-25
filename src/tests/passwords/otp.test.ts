import { describe, expect, it } from 'vitest'
import { parseOtpData, parseOtpUri } from '~/utils/passwords/otp'

describe('parseOtpData', () => {
  it('returns null for empty / undefined input', () => {
    expect(parseOtpData(null)).toBeNull()
    expect(parseOtpData(undefined)).toBeNull()
    expect(parseOtpData('')).toBeNull()
    expect(parseOtpData('   ')).toBeNull()
  })

  it('parses a bare secret with SHA1/6/30 defaults', () => {
    expect(parseOtpData('jbswy3dpehpk3pxp')).toEqual({
      secret: 'JBSWY3DPEHPK3PXP',
      digits: 6,
      period: 30,
      algorithm: 'SHA1',
    })
  })

  it('strips whitespace from bare secrets when requested', () => {
    expect(parseOtpData('JBSW Y3DP EHPK 3PXP', { stripWhitespace: true })).toEqual({
      secret: 'JBSWY3DPEHPK3PXP',
      digits: 6,
      period: 30,
      algorithm: 'SHA1',
    })
  })

  it('keeps whitespace in bare secrets by default', () => {
    // Bitwarden behaviour — secrets are exported without spaces but the parser
    // does not silently mangle whitespace it does see.
    expect(parseOtpData('JBSW Y3DP')).toEqual({
      secret: 'JBSW Y3DP',
      digits: 6,
      period: 30,
      algorithm: 'SHA1',
    })
  })

  it('parses an otpauth:// URI with explicit parameters', () => {
    const uri = 'otpauth://totp/Foo?secret=jbswy3dpehpk3pxp&digits=8&period=60&algorithm=SHA256'
    expect(parseOtpData(uri)).toEqual({
      secret: 'JBSWY3DPEHPK3PXP',
      digits: 8,
      period: 60,
      algorithm: 'SHA256',
    })
  })

  it('falls back to defaults for an otpauth URI without optional params', () => {
    const uri = 'otpauth://totp/Foo?secret=jbswy3dpehpk3pxp'
    expect(parseOtpData(uri)).toEqual({
      secret: 'JBSWY3DPEHPK3PXP',
      digits: 6,
      period: 30,
      algorithm: 'SHA1',
    })
  })
})

describe('parseOtpUri', () => {
  it('returns null for an otpauth URI missing the secret param', () => {
    expect(parseOtpUri('otpauth://totp/Foo?period=30')).toBeNull()
  })

  it('returns null for a malformed URI rather than throwing', () => {
    expect(parseOtpUri('not a uri')).toBeNull()
  })
})
