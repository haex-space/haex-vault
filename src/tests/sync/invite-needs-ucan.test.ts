/**
 * Tests for `inviteNeedsUcan`, the decision the realtime auto-finalize loop
 * makes for each accepted invite.
 *
 * The surrounding handler needs WebSocket + Tauri + Pinia and is covered
 * end-to-end by the e2e companion; here we lock down the pure predicate.
 *
 * Background: a UCAN is bearer-usable — nothing binds it to its presenter — so
 * the server stopped returning every invite's UCAN to every space member and
 * now emits `ucan` only to the invite's addressee, alongside a `hasUcan`
 * boolean. This loop runs on the *inviter's* device over invites addressed to
 * other members, so it must read the flag; reading the value would see
 * `undefined` on every redacted row and re-mint UCANs that already exist.
 *
 * Both field shapes are tolerated because client and server ship as separate
 * PRs, so either merge order briefly runs one without the other.
 */

import { describe, it, expect } from 'vitest'
import { inviteNeedsUcan } from '@/stores/sync/orchestrator/invite-finalize'

describe('inviteNeedsUcan — current server (emits hasUcan)', () => {
  it('does not re-mint when the server holds a UCAN it redacted from us', () => {
    // The inviter's own device listing an invite addressed to someone else.
    expect(inviteNeedsUcan({ hasUcan: true })).toBe(false)
  })

  it('mints for a token invite the server has no UCAN for', () => {
    expect(inviteNeedsUcan({ hasUcan: false })).toBe(true)
  })

  it('does not re-mint for our own invite, where the value is also present', () => {
    expect(inviteNeedsUcan({ hasUcan: true, ucan: 'eyJ...' })).toBe(false)
  })

  it('trusts the flag over an absent value', () => {
    expect(inviteNeedsUcan({ hasUcan: true, ucan: null })).toBe(false)
  })
})

describe('inviteNeedsUcan — server predating hasUcan', () => {
  it('falls back to the value when the flag is absent', () => {
    expect(inviteNeedsUcan({ ucan: 'eyJ...' })).toBe(false)
  })

  it('mints when neither flag nor value is present', () => {
    expect(inviteNeedsUcan({})).toBe(true)
  })

  it('mints when the old server reports an explicit null', () => {
    expect(inviteNeedsUcan({ ucan: null })).toBe(true)
  })
})
