// Part of `pnpm check:chat-state` (spec 072 FR-001): the time an entry of the chat's list shows.
import assert from 'node:assert/strict'
import { test } from 'node:test'

import { listTime, type ListTimeLabels } from '../src/lib/chat/listTime.ts'

const labels: ListTimeLabels = {
  now: 'Jetzt',
  minutes: (count) => `${count} Min.`,
  clock: (date) =>
    `${date.getHours()}:${String(date.getMinutes()).padStart(2, '0')}`,
  yesterday: 'Gestern',
  weekday: (date) => ['So', 'Mo', 'Di', 'Mi', 'Do', 'Fr', 'Sa'][date.getDay()]!,
  date: (date, withYear) =>
    `${date.getDate()}.${date.getMonth() + 1}.${withYear ? date.getFullYear() % 100 : ''}`,
}

// Thursday 2026-11-26, 14:30 local time.
const now = new Date(2026, 10, 26, 14, 30)
const at = (year: number, month: number, day: number, hour = 0, minute = 0) =>
  new Date(year, month, day, hour, minute).getTime()
const ago = (ms: number) => now.getTime() - ms
const show = (time: number) => listTime(time, labels, now)

test('under a minute reads as now, also for a clock a little ahead', () => {
  assert.equal(show(ago(0)), 'Jetzt')
  assert.equal(show(ago(59_000)), 'Jetzt')
  assert.equal(show(ago(-30_000)), 'Jetzt')
})

test('under an hour counts whole minutes', () => {
  assert.equal(show(ago(60_000)), '1 Min.')
  assert.equal(show(ago(48 * 60_000 + 20_000)), '48 Min.')
  assert.equal(show(ago(59 * 60_000 + 59_000)), '59 Min.')
})

test('the rest of today shows the clock time', () => {
  assert.equal(show(ago(60 * 60_000)), '13:30')
  assert.equal(show(at(2026, 10, 26, 0, 5)), '0:05')
})

test('an hour ago across midnight is yesterday, not today', () => {
  const justAfterMidnight = new Date(2026, 10, 26, 0, 20)
  const lateEvening = new Date(2026, 10, 25, 23, 10).getTime()
  assert.equal(listTime(lateEvening, labels, justAfterMidnight), 'Gestern')
})

test('yesterday, then the weekday for the rest of the week, then the date', () => {
  assert.equal(show(at(2026, 10, 25, 9, 0)), 'Gestern')
  assert.equal(show(at(2026, 10, 24, 9, 0)), 'Di')
  assert.equal(show(at(2026, 10, 20, 9, 0)), 'Fr')
  assert.equal(show(at(2026, 10, 19, 9, 0)), '19.11.')
  assert.equal(show(at(2026, 0, 3, 9, 0)), '3.1.')
})

test('another year adds the year to the date', () => {
  assert.equal(show(at(2025, 11, 31, 9, 0)), '31.12.25')
})
