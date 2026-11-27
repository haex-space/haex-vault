// Part of `pnpm check:chat-state`: the time a message of a conversation shows.
import assert from 'node:assert/strict'
import { test } from 'node:test'

import {
  dayLabel,
  dayStarts,
  messageDateTime,
  messageTime,
  type DayLabels,
} from '../src/lib/chat/messageTime.ts'

// Thursday 2026-11-26, 14:30 local time.
const now = new Date(2026, 10, 26, 14, 30)
const at = (month: number, day: number, hour: number, minute: number) =>
  new Date(2026, month, day, hour, minute).getTime()

test('a message of today shows the clock time only', () => {
  assert.equal(messageTime(at(10, 26, 1, 6), now, 'de'), '01:06')
  assert.equal(messageTime(at(10, 26, 14, 30), now, 'de'), '14:30')
})

test('an older message shows the short date and the clock time', () => {
  assert.equal(messageTime(at(10, 25, 23, 59), now, 'de'), '25.11.26, 23:59')
})

test('the tooltip names the date and the clock time', () => {
  assert.equal(messageDateTime(at(10, 26, 1, 6), 'de'), '26.11.2026, 01:06')
})

const labels: DayLabels = {
  today: 'Heute',
  yesterday: 'Gestern',
  date: (date) => `${date.getDate()}.${date.getMonth() + 1}.`,
}

test('a day is headed Today, Yesterday or by its date', () => {
  assert.equal(dayLabel(at(10, 26, 0, 1), now, labels), 'Heute')
  assert.equal(dayLabel(at(10, 25, 23, 59), now, labels), 'Gestern')
  assert.equal(dayLabel(at(10, 20, 12, 0), now, labels), '20.11.')
})

test('the first message and every change of day open a day', () => {
  const m = (id: string, month: number, day: number, hour: number) => ({
    id,
    createdAt: at(month, day, hour, 0),
  })
  const starts = dayStarts([
    m('a', 10, 25, 9),
    m('b', 10, 25, 23),
    m('c', 10, 26, 0),
    m('d', 10, 26, 14),
  ])
  assert.deepEqual([...starts], ['a', 'c'])
  assert.equal(dayStarts([]).size, 0)
})
