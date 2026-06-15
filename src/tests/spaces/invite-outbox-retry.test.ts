/**
 * Tests for the invite-outbox retry state machine.
 *
 * The full `processOutboxAsync` pipeline needs DB + Tauri + Pinia and is
 * exercised end-to-end by the e2e companion. Here we lock down the pure
 * state transition that decides — given an entry's current retry count and
 * the outcome of a `local_delivery_push_invite` attempt — whether the
 * outbox row should move to DELIVERED, stay PENDING with backoff, or
 * surface to the user as FAILED.
 *
 * Regression targets:
 * - D2 from docs/plans/code-review-followup.md: `accepted === false` used
 *   to stay PENDING without bumping retryCount, causing tick-loop spam.
 * - 2026-06-15 invite-outbox resilience plan: transient failures now stay
 *   PENDING forever (until `expiresAt`); only permanent failures land in
 *   FAILED. Backoff is capped, not gated by a retry count.
 */

import { describe, it, expect } from 'vitest'
import {
  computeOutboxNextState,
  type OutboxAttemptOutcome,
} from '@/composables/useInviteOutbox'
import { OutboxStatus } from '@/database/constants'

const NOW = Date.parse('2026-06-04T12:00:00.000Z')

describe('computeOutboxNextState', () => {
  it('marks a delivered attempt as DELIVERED', () => {
    const outcome: OutboxAttemptOutcome = { delivered: true }
    expect(computeOutboxNextState(0, outcome, NOW)).toEqual({
      status: OutboxStatus.DELIVERED,
    })
  })

  it('rejected attempt (accepted=false, transient) increments retryCount and schedules a backoff', () => {
    const outcome: OutboxAttemptOutcome = {
      delivered: false,
      error: 'PushInvite rejected by recipient (accepted=false)',
      transient: true,
    }
    const next = computeOutboxNextState(0, outcome, NOW)
    expect(next.status).toBe(OutboxStatus.PENDING)
    expect(next.retryCount).toBe(1)
    expect(next.lastError).toBe('PushInvite rejected by recipient (accepted=false)')
    expect(next.nextRetryAt).toBeDefined()
    expect(Date.parse(next.nextRetryAt!)).toBeGreaterThan(NOW)
  })

  it('thrown transient error path produces the same shape as a rejected attempt', () => {
    const next = computeOutboxNextState(
      0,
      { delivered: false, error: 'connect timeout', transient: true },
      NOW,
    )
    expect(next.status).toBe(OutboxStatus.PENDING)
    expect(next.retryCount).toBe(1)
    expect(next.lastError).toBe('connect timeout')
    expect(next.nextRetryAt).toBeDefined()
  })

  it('permanent failure transitions to FAILED on the first attempt', () => {
    const outcome: OutboxAttemptOutcome = {
      delivered: false,
      error: 'Remote error: UCAN audience mismatch',
      transient: false,
    }
    const next = computeOutboxNextState(0, outcome, NOW)
    expect(next.status).toBe(OutboxStatus.FAILED)
    expect(next.retryCount).toBe(1)
    expect(next.lastError).toBe('Remote error: UCAN audience mismatch')
    // FAILED rows are not scheduled for another retry.
    expect(next.nextRetryAt).toBeUndefined()
  })

  it('transient failure stays PENDING even at very high retry counts', () => {
    // The plan removes the retry-count terminator: as long as the outcome
    // is transient, the row stays PENDING. expiresAt is the only liveness
    // terminator now (handled by processOutboxAsync, not this function).
    const outcome: OutboxAttemptOutcome = {
      delivered: false,
      error: 'still offline',
      transient: true,
    }
    const next = computeOutboxNextState(99, outcome, NOW)
    expect(next.status).toBe(OutboxStatus.PENDING)
    expect(next.retryCount).toBe(100)
    expect(next.nextRetryAt).toBeDefined()
  })

  it('backoff grows with retry count, then caps', () => {
    const transient: OutboxAttemptOutcome = {
      delivered: false,
      error: 'e',
      transient: true,
    }
    const early = computeOutboxNextState(0, transient, NOW)
    const mid = computeOutboxNextState(3, transient, NOW)
    const late = computeOutboxNextState(20, transient, NOW)

    // Monotonic up to the cap.
    expect(Date.parse(mid.nextRetryAt!)).toBeGreaterThanOrEqual(Date.parse(early.nextRetryAt!))
    expect(Date.parse(late.nextRetryAt!)).toBeGreaterThanOrEqual(Date.parse(mid.nextRetryAt!))

    // Cap is 3600s — at retryCount=20 the schedule is exactly the cap, so
    // a hypothetical retryCount=21 must produce the same delay (not larger).
    const sameAsCap = computeOutboxNextState(21, transient, NOW)
    expect(Date.parse(sameAsCap.nextRetryAt!)).toBe(Date.parse(late.nextRetryAt!))
  })
})
