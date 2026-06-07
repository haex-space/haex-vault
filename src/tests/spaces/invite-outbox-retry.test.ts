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
 * Regression target: D2 from docs/plans/code-review-followup.md — when
 * `accepted === false`, the row used to stay PENDING with no retryCount
 * bump and no nextRetryAt advance, so the processor would re-pick it on
 * every tick forever.
 */

import { describe, it, expect } from 'vitest'
import {
  computeOutboxNextState,
  MAX_OUTBOX_RETRIES,
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

  it('rejected attempt (accepted=false) increments retryCount and schedules a backoff', () => {
    const outcome: OutboxAttemptOutcome = {
      delivered: false,
      error: 'PushInvite rejected by recipient (accepted=false)',
    }
    const next = computeOutboxNextState(0, outcome, NOW)
    expect(next.status).toBe(OutboxStatus.PENDING)
    expect(next.retryCount).toBe(1)
    expect(next.lastError).toBe('PushInvite rejected by recipient (accepted=false)')
    expect(next.nextRetryAt).toBeDefined()
    // Backoff must move nextRetryAt strictly into the future.
    expect(Date.parse(next.nextRetryAt!)).toBeGreaterThan(NOW)
  })

  it('thrown error path produces the same shape as a rejected attempt', () => {
    const next = computeOutboxNextState(
      0,
      { delivered: false, error: 'boom' },
      NOW,
    )
    expect(next.status).toBe(OutboxStatus.PENDING)
    expect(next.retryCount).toBe(1)
    expect(next.lastError).toBe('boom')
    expect(next.nextRetryAt).toBeDefined()
  })

  it('transitions to FAILED on the retry that would exceed MAX_OUTBOX_RETRIES', () => {
    const outcome: OutboxAttemptOutcome = {
      delivered: false,
      error: 'permanently unreachable',
    }
    const next = computeOutboxNextState(MAX_OUTBOX_RETRIES - 1, outcome, NOW)
    expect(next.status).toBe(OutboxStatus.FAILED)
    expect(next.retryCount).toBe(MAX_OUTBOX_RETRIES)
    expect(next.lastError).toBe('permanently unreachable')
    // FAILED rows are not scheduled for another retry.
    expect(next.nextRetryAt).toBeUndefined()
  })

  it('still produces FAILED when retryCount is somehow already at MAX', () => {
    const outcome: OutboxAttemptOutcome = { delivered: false, error: 'x' }
    const next = computeOutboxNextState(MAX_OUTBOX_RETRIES, outcome, NOW)
    expect(next.status).toBe(OutboxStatus.FAILED)
  })

  it('backoff grows with retry count', () => {
    const early = computeOutboxNextState(
      0,
      { delivered: false, error: 'e' },
      NOW,
    )
    const later = computeOutboxNextState(
      3,
      { delivered: false, error: 'e' },
      NOW,
    )
    expect(Date.parse(later.nextRetryAt!)).toBeGreaterThanOrEqual(
      Date.parse(early.nextRetryAt!),
    )
  })
})
