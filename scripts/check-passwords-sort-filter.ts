// Part of `pnpm check:passwords` (spec 092-wallet-passes, FR-021): the date sort
// (src/lib/passwords/sortFilter.ts).
import assert from 'node:assert/strict'
import { test } from 'node:test'

import {
  filterByKind,
  hideExpired,
  sortByDate,
  type SortableHeader,
} from '../src/lib/passwords/sortFilter.ts'

const NOW = new Date(2030, 4, 10, 12, 0, 0) // 10 May 2030, noon, in the device's zone

const pass = (
  id: string,
  title: string,
  extra: Partial<SortableHeader> = {},
): SortableHeader => ({ id, title, kind: 'pass', ...extra })

const at = (days: number, hour = 20): string =>
  new Date(2030, 4, 10 + days, hour, 0, 0).toISOString()

const ids = (headers: readonly SortableHeader[]) => headers.map((h) => h.id)

test('upcoming first with the nearest on top, then over with the most recent first, then undated', () => {
  const sorted = sortByDate(
    [
      pass('undated', 'A'),
      pass('past-old', 'B', { startAt: at(-10), startZone: '+00:00' }),
      pass('soon', 'C', { startAt: at(1), startZone: '+00:00' }),
      pass('past-new', 'D', { startAt: at(-1), startZone: '+00:00' }),
      pass('later', 'E', { startAt: at(30), startZone: '+00:00' }),
    ],
    NOW,
  )
  assert.deepEqual(ids(sorted), [
    'soon',
    'later',
    'past-new',
    'past-old',
    'undated',
  ])
})

test('ties go by title and the input is not changed', () => {
  const input = [
    pass('b', 'Beta', { startAt: at(2), startZone: '+00:00' }),
    pass('a', 'alpha', { startAt: at(2), startZone: '+00:00' }),
    pass('n', 'No date 10'),
    pass('m', 'No date 9'),
  ]
  const copy = [...input]
  assert.deepEqual(ids(sortByDate(input, NOW)), ['a', 'b', 'm', 'n'])
  assert.deepEqual(input, copy)
})

test('a date alone sorts at midnight of the device and is upcoming through its day', () => {
  const sorted = sortByDate(
    [
      pass('yesterday', 'Y', { startDate: '2030-05-09' }),
      pass('today', 'T', { startDate: '2030-05-10' }),
      pass('moment-today', 'M', { startAt: at(0, 18), startZone: '+00:00' }),
      pass('tomorrow', 'U', { startDate: '2030-05-11' }),
    ],
    NOW,
  )
  // Today's date counts as upcoming although midnight has passed; it sorts before the evening.
  assert.deepEqual(ids(sorted), [
    'today',
    'moment-today',
    'tomorrow',
    'yesterday',
  ])
})

test('a login is dated by its expiry date and so is a pass without a start (its end)', () => {
  const sorted = sortByDate(
    [
      { id: 'login-far', title: 'L1', kind: 'login', expiresAt: '2031-01-01' },
      { id: 'login-none', title: 'L2', kind: 'login' },
      { id: 'login-gone', title: 'L3', expiresAt: '2020-01-01' },
      pass('pass-expiry-only', 'P', { expiresAt: '2030-06-01' }),
    ],
    NOW,
  )
  // The pass without a start sorts by its end, 2030-06-01, so it comes before the far login.
  assert.deepEqual(ids(sorted), [
    'pass-expiry-only',
    'login-far',
    'login-gone',
    'login-none',
  ])
})

test('an invalid date is undated and hundreds of entries sort', () => {
  assert.deepEqual(
    ids(sortByDate([pass('x', 'X', { startDate: 'soon' })], NOW)),
    ['x'],
  )
  // A day that does not exist does not roll over into the next month or year: it is undated.
  assert.deepEqual(
    ids(
      sortByDate(
        [
          pass('rolled', 'A', { startDate: '2030-13-01' }),
          pass('feb30', 'B', { expiresAt: '2030-02-30' }),
          pass('dated', 'C', { startDate: '2030-06-01' }),
        ],
        NOW,
      ),
    ),
    ['dated', 'rolled', 'feb30'],
  )
  const many = Array.from({ length: 500 }, (_, i) =>
    pass(`p${i}`, `T${i}`, { startAt: at(i - 250), startZone: '+00:00' }),
  )
  const sorted = sortByDate(many, NOW)
  assert.equal(sorted.length, 500)
  assert.equal(sorted[0]?.id, 'p250')
})

const KINDS = [
  'ticket',
  'loyalty_card',
  'voucher',
  'membership',
  'boarding_pass',
  'other',
]

test('filterByKind keeps one kind, null keeps all, an unknown kind counts as other', () => {
  const all = [
    { id: 'a', passKind: 'ticket' },
    { id: 'b', passKind: 'voucher' },
    { id: 'c', passKind: 'ticket' },
    { id: 'd', passKind: 'hologram' }, // written by a newer holzi
    { id: 'e', passKind: null },
    { id: 'f' },
  ]
  assert.deepEqual(
    filterByKind(all, 'ticket', KINDS).map((h) => h.id),
    ['a', 'c'],
  )
  assert.deepEqual(
    filterByKind(all, 'other', KINDS).map((h) => h.id),
    ['d', 'e', 'f'],
  )
  assert.equal(filterByKind(all, null, KINDS).length, 6)
  assert.deepEqual(filterByKind(all, 'membership', KINDS), [])
})

test('hideExpired drops what is over, keeps today, undated and invalid ends', () => {
  const day = (offset: number) => {
    const d = new Date(
      NOW.getFullYear(),
      NOW.getMonth(),
      NOW.getDate() + offset,
    )
    const pad = (n: number) => String(n).padStart(2, '0')
    return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`
  }
  const all = [
    { id: 'past', expiresAt: day(-1) },
    { id: 'today', expiresAt: day(0) },
    { id: 'future', expiresAt: day(30) },
    { id: 'none' },
    { id: 'null', expiresAt: null },
    { id: 'bad', expiresAt: '2030-13-45' },
  ]
  assert.deepEqual(
    hideExpired(all, NOW).map((h) => h.id),
    ['today', 'future', 'none', 'null', 'bad'],
  )
})

test('kind, hide expired and sort combine in any order to the same list', () => {
  const headers = [
    pass('t1', 'Concert', { startAt: at(5) }),
    pass('t2', 'Old gig', { startAt: at(-30), expiresAt: '2030-04-01' }),
    pass('v1', 'Voucher', { expiresAt: '2030-12-31' }),
  ].map((h) => ({
    ...h,
    passKind: h.id.startsWith('t') ? 'ticket' : 'voucher',
  }))
  const a = sortByDate(
    hideExpired(filterByKind(headers, 'ticket', KINDS), NOW),
    NOW,
  )
  const b = hideExpired(
    sortByDate(filterByKind(headers, 'ticket', KINDS), NOW),
    NOW,
  )
  assert.deepEqual(
    a.map((h) => h.id),
    ['t1'],
  )
  assert.deepEqual(
    b.map((h) => h.id),
    ['t1'],
  )
})
