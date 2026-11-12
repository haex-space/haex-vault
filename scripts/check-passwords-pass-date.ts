// Part of `pnpm check:passwords` (spec 092-wallet-passes, FR-002, T025): the start of a pass and
// its time zones (src/lib/passwords/passDate.ts). Every zone is passed in, so the result never
// depends on the zone of the machine; only `isExpiredPass` reads the calendar of the process zone,
// and its tests set `TZ` themselves.
import assert from 'node:assert/strict'
import { test } from 'node:test'

import {
  formatStart,
  instantToWallTime,
  isExpiredPass,
  startInstant,
  wallTimeToInstant,
} from '../src/lib/passwords/passDate.ts'

const LONDON = 'Europe/London'
const BERLIN = 'Europe/Berlin'
const NEW_YORK = 'America/New_York'

/** The whole result as the strings a person reads, in a fixed locale. */
const show = (startAt: string, startZone: string, deviceZone: string) =>
  formatStart({ startAt, startZone }, deviceZone, 'en-GB')

test('20:00 in London is 20:00 for London and 21:00 on a Berlin device, both labelled', () => {
  const shown = formatStart(
    { startAt: '2026-11-14T20:00:00.000Z', startZone: LONDON },
    BERLIN,
    'en-GB',
  )
  assert.ok(shown)
  assert.match(shown.place, /14 Nov 2026/)
  assert.match(shown.place, /20:00 Europe\/London$/)
  assert.ok(shown.device)
  assert.match(shown.device, /14 Nov 2026/)
  assert.match(shown.device, /21:00 Europe\/Berlin$/)
})

test('on a London device there is only the time of the place', () => {
  const shown = show('2026-11-14T20:00:00.000Z', LONDON, LONDON)
  assert.ok(shown)
  assert.match(shown.place, /20:00 Europe\/London$/)
  assert.equal(shown.device, undefined)
})

test('the device part moves to the next day when the device is a day ahead', () => {
  const shown = show('2026-11-14T23:30:00.000Z', LONDON, 'Asia/Tokyo')
  assert.ok(shown)
  assert.match(shown.place, /14 Nov 2026/)
  assert.match(shown.place, /23:30 Europe\/London$/)
  assert.ok(shown.device)
  assert.match(shown.device, /15 Nov 2026/)
  assert.match(shown.device, /08:30 Asia\/Tokyo$/)
})

test('a fixed offset is the time of the place', () => {
  const shown = show('2026-11-14T18:00:00.000Z', '+02:00', LONDON)
  assert.ok(shown)
  assert.match(shown.place, /20:00 UTC\+02:00$/)
  assert.ok(shown.device)
  assert.match(shown.device, /18:00 Europe\/London$/)

  const half = show('2026-11-14T18:00:00.000Z', '-05:30', 'UTC')
  assert.ok(half)
  assert.match(half.place, /12:30 UTC-05:30$/)
  assert.match(half.device ?? '', /18:00 UTC$/)
})

test('a fixed offset with the offset of the device shows no device part, by the offset at that instant', () => {
  // +01:00 is Berlin in winter, but in summer Berlin is +02:00.
  const winter = show('2026-01-20T12:00:00.000Z', '+01:00', BERLIN)
  assert.ok(winter)
  assert.match(winter.place, /13:00 UTC\+01:00$/)
  assert.equal(winter.device, undefined)

  const summer = show('2026-07-20T12:00:00.000Z', '+01:00', BERLIN)
  assert.ok(summer)
  assert.match(summer.place, /13:00 UTC\+01:00$/)
  assert.ok(summer.device)
  assert.match(summer.device, /14:00 Europe\/Berlin$/)
})

test('a date alone is a date, with no zone and no device part, on any device', () => {
  for (const zone of [
    LONDON,
    'Pacific/Kiritimati',
    'Pacific/Pago_Pago',
    'UTC',
  ]) {
    const shown = formatStart({ startDate: '2026-11-14' }, zone, 'en-GB')
    assert.deepEqual(shown, { place: '14 Nov 2026' })
  }
})

test('Europe: the clocks go forward on 2026-03-29 at 01:00 UTC and back on 2026-10-25', () => {
  // London and Berlin change at the same instant, so the difference stays one hour...
  for (const at of [
    '2026-03-29T00:59:00.000Z',
    '2026-03-29T01:00:00.000Z',
    '2026-10-25T00:59:00.000Z',
    '2026-10-25T01:00:00.000Z',
  ]) {
    const shown = show(at, LONDON, BERLIN)
    assert.ok(shown)
    assert.ok(shown.device, at)
  }
  // ...and the time of the place itself follows the change.
  assert.match(
    show('2026-03-29T00:59:00.000Z', LONDON, LONDON)!.place,
    /00:59 Europe\/London$/,
  )
  assert.match(
    show('2026-03-29T01:00:00.000Z', LONDON, LONDON)!.place,
    /02:00 Europe\/London$/,
  )
  assert.match(
    show('2026-10-25T00:59:00.000Z', LONDON, LONDON)!.place,
    /01:59 Europe\/London$/,
  )
  assert.match(
    show('2026-10-25T01:00:00.000Z', LONDON, LONDON)!.place,
    /01:00 Europe\/London$/,
  )
  assert.match(
    show('2026-03-29T00:59:00.000Z', LONDON, BERLIN)!.device!,
    /01:59 Europe\/Berlin$/,
  )
  assert.match(
    show('2026-03-29T01:00:00.000Z', LONDON, BERLIN)!.device!,
    /03:00 Europe\/Berlin$/,
  )
  assert.match(
    show('2026-10-25T00:59:00.000Z', LONDON, BERLIN)!.device!,
    /02:59 Europe\/Berlin$/,
  )
  assert.match(
    show('2026-10-25T01:00:00.000Z', LONDON, BERLIN)!.device!,
    /02:00 Europe\/Berlin$/,
  )
})

test('North America: the clocks go forward on 2026-03-08 and back on 2026-11-01', () => {
  // New York changes at 07:00 UTC on 03-08 and at 06:00 UTC on 11-01.
  const cases: [string, RegExp, RegExp][] = [
    [
      '2026-03-08T06:59:00.000Z',
      /01:59 America\/New_York$/,
      /06:59 Europe\/London$/,
    ],
    [
      '2026-03-08T07:00:00.000Z',
      /03:00 America\/New_York$/,
      /07:00 Europe\/London$/,
    ],
    [
      '2026-11-01T05:59:00.000Z',
      /01:59 America\/New_York$/,
      /05:59 Europe\/London$/,
    ],
    [
      '2026-11-01T06:00:00.000Z',
      /01:00 America\/New_York$/,
      /06:00 Europe\/London$/,
    ],
  ]
  for (const [at, place, device] of cases) {
    const shown = show(at, NEW_YORK, LONDON)
    assert.ok(shown, at)
    assert.match(shown.place, place, at)
    assert.ok(shown.device, at)
    assert.match(shown.device, device, at)
  }
})

test('between the two changes the difference between New York and Berlin is not the usual six hours', () => {
  // 2026-03-08 to 2026-03-29 the United States are on summer time and Europe is not yet: 5 hours.
  const gap = show('2026-03-15T17:00:00.000Z', NEW_YORK, BERLIN)
  assert.ok(gap)
  assert.match(gap.place, /13:00 America\/New_York$/)
  assert.match(gap.device!, /18:00 Europe\/Berlin$/)
  // And 2026-10-25 to 2026-11-01 the other way round.
  const back = show('2026-10-28T17:00:00.000Z', NEW_YORK, BERLIN)
  assert.ok(back)
  assert.match(back.place, /13:00 America\/New_York$/)
  assert.match(back.device!, /18:00 Europe\/Berlin$/)
  // Outside of those weeks it is six.
  const usual = show('2026-06-15T17:00:00.000Z', NEW_YORK, BERLIN)
  assert.ok(usual)
  assert.match(usual.place, /13:00 America\/New_York$/)
  assert.match(usual.device!, /19:00 Europe\/Berlin$/)
})

test('a place without daylight saving time is compared at the instant, not by name', () => {
  // Berlin is +01:00 in winter and +02:00 in summer; Africa/Lagos is +01:00 all year.
  const winter = show('2026-01-15T12:00:00.000Z', 'Africa/Lagos', BERLIN)
  assert.ok(winter)
  assert.equal(winter.device, undefined)
  const summer = show('2026-07-15T12:00:00.000Z', 'Africa/Lagos', BERLIN)
  assert.ok(summer)
  assert.match(summer.device!, /14:00 Europe\/Berlin$/)
})

test('an unknown zone name is shown as UTC with the name, and never throws', () => {
  const shown = show('2026-11-14T20:00:00.000Z', 'Nowhere/Land', 'UTC')
  assert.ok(shown)
  assert.match(shown.place, /20:00 UTC \(Nowhere\/Land\)$/)
  assert.equal(shown.device, undefined)

  const other = show('2026-11-14T20:00:00.000Z', 'Nowhere/Land', BERLIN)
  assert.ok(other)
  assert.match(other.place, /20:00 UTC \(Nowhere\/Land\)$/)
  assert.match(other.device!, /21:00 Europe\/Berlin$/)

  // An unknown zone on the device is the same fallback.
  const device = show('2026-11-14T20:00:00.000Z', LONDON, 'Nowhere/Land')
  assert.ok(device)
  assert.equal(device.device, undefined)
  const berlinPlace = show('2026-11-14T20:00:00.000Z', BERLIN, 'Nowhere/Land')
  assert.match(berlinPlace!.device!, /20:00 UTC \(Nowhere\/Land\)$/)

  // A missing zone is UTC.
  const none = formatStart(
    { startAt: '2026-11-14T20:00:00.000Z' },
    'UTC',
    'en-GB',
  )
  assert.ok(none)
  assert.match(none.place, /20:00 UTC$/)
})

test('invalid input is no start, never an error', () => {
  const bad = [
    { startAt: 'tomorrow', startZone: LONDON },
    { startAt: '2026-11-14 20:00', startZone: LONDON },
    { startAt: '2026-11-14T25:00:00.000Z', startZone: LONDON },
    { startAt: '2026-02-30T10:00:00.000Z', startZone: LONDON },
    { startDate: '14.11.2026' },
    { startDate: '2026-13-01' },
    { startDate: '2026-02-29' },
    {},
    { startAt: null, startDate: null, startZone: null },
    { startAt: '', startDate: '' },
  ]
  for (const header of bad) {
    assert.equal(
      formatStart(header, BERLIN, 'en-GB'),
      null,
      JSON.stringify(header),
    )
    assert.equal(startInstant(header), null, JSON.stringify(header))
  }
  assert.equal(formatStart(null, BERLIN), null)
  assert.equal(formatStart(undefined, BERLIN), null)
  assert.equal(startInstant(undefined), null)
  assert.equal(
    startInstant({
      startAt: '2026-11-14T20:00:00.000Z',
      startZone: 'Nowhere/Land',
    }),
    Date.UTC(2026, 10, 14, 20),
  )
})

test('startInstant: a moment is its instant, a date alone is the UTC midnight of the date', () => {
  assert.equal(
    startInstant({ startAt: '2026-11-14T20:00:00.000Z' }),
    Date.UTC(2026, 10, 14, 20),
  )
  assert.equal(
    startInstant({ startAt: '2026-11-14T21:00:00.000+01:00' }),
    Date.UTC(2026, 10, 14, 20),
  )
  assert.equal(
    startInstant({ startDate: '2026-11-14' }),
    Date.UTC(2026, 10, 14),
  )
  assert.equal(startInstant({ startDate: '2024-02-29' }), Date.UTC(2024, 1, 29))
  // The same wall time in two zones is two instants; the zone is not part of it.
  assert.equal(
    startInstant({ startAt: '2026-11-14T20:00:00.000Z', startZone: BERLIN }),
    startInstant({ startAt: '2026-11-14T20:00:00.000Z', startZone: NEW_YORK }),
  )
  // A date alone sorts before a moment on the same day, and an earlier day before both.
  assert.ok(
    startInstant({ startDate: '2026-11-14' })! <
      startInstant({ startAt: '2026-11-14T00:30:00.000Z' })!,
  )
  assert.ok(
    startInstant({ startDate: '2026-11-13' })! <
      startInstant({ startDate: '2026-11-14' })!,
  )
})

test('an end before the start is allowed', () => {
  const header = { startDate: '2026-11-14', expiresAt: '2026-11-01' }
  assert.doesNotThrow(() => formatStart(header, BERLIN))
  assert.doesNotThrow(() => startInstant(header))
  assert.doesNotThrow(() => isExpiredPass(header, new Date(2026, 10, 12, 12)))
  assert.equal(isExpiredPass(header, new Date(2026, 10, 12, 12)), true)
  assert.equal(isExpiredPass(header, new Date(2026, 9, 31, 12)), false)
})

/** Runs the body with the process zone set, then puts it back. */
function inZone(zone: string, body: () => void) {
  const saved = process.env.TZ
  process.env.TZ = zone
  try {
    body()
  } finally {
    if (saved === undefined) {
      delete process.env.TZ
    } else {
      process.env.TZ = saved
    }
  }
}

test('a pass is over when its date is before today, a pass of today is not', () => {
  for (const zone of [
    'UTC',
    BERLIN,
    NEW_YORK,
    'Pacific/Kiritimati',
    'Pacific/Pago_Pago',
  ]) {
    inZone(zone, () => {
      const noon = new Date(2026, 10, 14, 12, 0)
      const lateEvening = new Date(2026, 10, 14, 23, 59, 59)
      const earlyMorning = new Date(2026, 10, 14, 0, 0, 1)
      for (const now of [noon, lateEvening, earlyMorning]) {
        assert.equal(
          isExpiredPass({ expiresAt: '2026-11-14' }, now),
          false,
          zone,
        )
        assert.equal(
          isExpiredPass({ expiresAt: '2026-11-15' }, now),
          false,
          zone,
        )
        assert.equal(
          isExpiredPass({ expiresAt: '2026-11-13' }, now),
          true,
          zone,
        )
        assert.equal(
          isExpiredPass({ expiresAt: '2025-12-31' }, now),
          true,
          zone,
        )
      }
      const nextDay = new Date(2026, 10, 15, 0, 0, 1)
      assert.equal(
        isExpiredPass({ expiresAt: '2026-11-14' }, nextDay),
        true,
        zone,
      )
    })
  }
})

test('today is the calendar day of the device, not the day in UTC', () => {
  // 2026-11-14 23:30 in Auckland (+13:00) is still 2026-11-14 10:30 UTC; 2026-11-14 01:00 in
  // New York is already 06:00 UTC on the same day, and 2026-11-14 22:00 in New York is
  // 03:00 UTC on the 15th.
  inZone('America/New_York', () => {
    const now = new Date('2026-11-15T03:00:00.000Z')
    assert.equal(now.getDate(), 14)
    assert.equal(isExpiredPass({ expiresAt: '2026-11-14' }, now), false)
    assert.equal(isExpiredPass({ expiresAt: '2026-11-13' }, now), true)
  })
  inZone('Pacific/Auckland', () => {
    const now = new Date('2026-11-14T12:00:00.000Z')
    assert.equal(now.getDate(), 15)
    assert.equal(isExpiredPass({ expiresAt: '2026-11-14' }, now), true)
    assert.equal(isExpiredPass({ expiresAt: '2026-11-15' }, now), false)
  })
})

test('the days of the changes count as ordinary days for the expiry date', () => {
  for (const zone of [BERLIN, NEW_YORK]) {
    inZone(zone, () => {
      const cases: [string, string][] = [
        ['2026-03-08', '2026-03-07'],
        ['2026-03-29', '2026-03-28'],
        ['2026-10-25', '2026-10-24'],
        ['2026-11-01', '2026-10-31'],
      ]
      for (const [day, before] of cases) {
        const [y, m, d] = day.split('-').map(Number)
        const now = new Date(y!, m! - 1, d!, 12)
        assert.equal(
          isExpiredPass({ expiresAt: day }, now),
          false,
          `${zone} ${day}`,
        )
        assert.equal(
          isExpiredPass({ expiresAt: before }, now),
          true,
          `${zone} ${day}`,
        )
      }
    })
  }
})

test('no end, or an end that is no date, never expires', () => {
  const now = new Date(2026, 10, 14, 12)
  assert.equal(isExpiredPass({}, now), false)
  assert.equal(isExpiredPass({ expiresAt: null }, now), false)
  assert.equal(isExpiredPass({ expiresAt: '' }, now), false)
  assert.equal(isExpiredPass({ expiresAt: 'soon' }, now), false)
  assert.equal(isExpiredPass({ expiresAt: '2026-02-30' }, now), false)
  assert.equal(isExpiredPass(undefined, now), false)
})

test('wallTimeToInstant: the wall clock of the place becomes a UTC instant', () => {
  assert.equal(
    wallTimeToInstant('2026-11-14T20:00', LONDON),
    '2026-11-14T20:00:00.000Z',
  )
  assert.equal(
    wallTimeToInstant('2026-11-14T21:00', BERLIN),
    '2026-11-14T20:00:00.000Z',
  )
  assert.equal(
    wallTimeToInstant('2026-07-14T22:00', BERLIN),
    '2026-07-14T20:00:00.000Z',
  )
  assert.equal(
    wallTimeToInstant('2026-11-14T15:00', NEW_YORK),
    '2026-11-14T20:00:00.000Z',
  )
  assert.equal(
    wallTimeToInstant('2026-11-14T22:00', '+02:00'),
    '2026-11-14T20:00:00.000Z',
  )
  assert.equal(
    wallTimeToInstant('2026-11-14T14:30', '-05:30'),
    '2026-11-14T20:00:00.000Z',
  )
  assert.equal(
    wallTimeToInstant('2026-11-14T20:00', 'UTC'),
    '2026-11-14T20:00:00.000Z',
  )
})

test('wallTimeToInstant: an unknown zone counts as UTC and never throws', () => {
  assert.equal(
    wallTimeToInstant('2026-11-14T20:00', 'Nowhere/Land'),
    '2026-11-14T20:00:00.000Z',
  )
  assert.equal(
    wallTimeToInstant('2026-11-14T20:00', ''),
    '2026-11-14T20:00:00.000Z',
  )
  assert.equal(
    wallTimeToInstant('2026-11-14T20:00', null),
    '2026-11-14T20:00:00.000Z',
  )
})

test('wallTimeToInstant: text that is no wall clock reading is null', () => {
  for (const text of [
    '',
    'noon',
    '2026-11-14',
    '2026-11-14 20:00',
    '2026-11-14T20:00:00.000Z',
    '2026-11-14T24:00',
    '2026-11-14T20:60',
    '2026-02-30T10:00',
    '2026-13-01T10:00',
  ]) {
    assert.equal(wallTimeToInstant(text, LONDON), null, text)
  }
})

test('wallTimeToInstant: a reading in the gap of the spring change is the first instant after it', () => {
  // Berlin: 02:00 to 03:00 does not exist on 2026-03-29 (the change is at 01:00 UTC).
  for (const wall of [
    '2026-03-29T02:00',
    '2026-03-29T02:30',
    '2026-03-29T02:59',
  ]) {
    assert.equal(
      wallTimeToInstant(wall, BERLIN),
      '2026-03-29T01:00:00.000Z',
      wall,
    )
  }
  assert.equal(
    wallTimeToInstant('2026-03-29T01:59', BERLIN),
    '2026-03-29T00:59:00.000Z',
  )
  assert.equal(
    wallTimeToInstant('2026-03-29T03:00', BERLIN),
    '2026-03-29T01:00:00.000Z',
  )
  // New York: 02:00 to 03:00 does not exist on 2026-03-08 (the change is at 07:00 UTC).
  for (const wall of [
    '2026-03-08T02:00',
    '2026-03-08T02:30',
    '2026-03-08T02:59',
  ]) {
    assert.equal(
      wallTimeToInstant(wall, NEW_YORK),
      '2026-03-08T07:00:00.000Z',
      wall,
    )
  }
  assert.equal(
    wallTimeToInstant('2026-03-08T01:59', NEW_YORK),
    '2026-03-08T06:59:00.000Z',
  )
  assert.equal(
    wallTimeToInstant('2026-03-08T03:00', NEW_YORK),
    '2026-03-08T07:00:00.000Z',
  )
})

test('wallTimeToInstant: a reading that occurs twice in the autumn change is the earlier one', () => {
  // Berlin: 02:00 to 03:00 happens twice on 2026-10-25 (the change is at 01:00 UTC).
  assert.equal(
    wallTimeToInstant('2026-10-25T02:00', BERLIN),
    '2026-10-25T00:00:00.000Z',
  )
  assert.equal(
    wallTimeToInstant('2026-10-25T02:30', BERLIN),
    '2026-10-25T00:30:00.000Z',
  )
  assert.equal(
    wallTimeToInstant('2026-10-25T01:59', BERLIN),
    '2026-10-24T23:59:00.000Z',
  )
  assert.equal(
    wallTimeToInstant('2026-10-25T03:00', BERLIN),
    '2026-10-25T02:00:00.000Z',
  )
  // New York: 01:00 to 02:00 happens twice on 2026-11-01 (the change is at 06:00 UTC).
  assert.equal(
    wallTimeToInstant('2026-11-01T01:00', NEW_YORK),
    '2026-11-01T05:00:00.000Z',
  )
  assert.equal(
    wallTimeToInstant('2026-11-01T01:30', NEW_YORK),
    '2026-11-01T05:30:00.000Z',
  )
  assert.equal(
    wallTimeToInstant('2026-11-01T00:59', NEW_YORK),
    '2026-11-01T04:59:00.000Z',
  )
  assert.equal(
    wallTimeToInstant('2026-11-01T02:00', NEW_YORK),
    '2026-11-01T07:00:00.000Z',
  )
})

test('instantToWallTime: the wall clock of the place, and null for no instant', () => {
  assert.equal(
    instantToWallTime('2026-11-14T20:00:00.000Z', BERLIN),
    '2026-11-14T21:00',
  )
  assert.equal(
    instantToWallTime('2026-07-14T20:00:00.000Z', BERLIN),
    '2026-07-14T22:00',
  )
  assert.equal(
    instantToWallTime('2026-11-14T20:00:00.000Z', NEW_YORK),
    '2026-11-14T15:00',
  )
  assert.equal(
    instantToWallTime('2026-11-14T20:00:00.000Z', '+02:00'),
    '2026-11-14T22:00',
  )
  assert.equal(
    instantToWallTime('2026-11-14T23:30:00.000Z', 'Asia/Tokyo'),
    '2026-11-15T08:30',
  )
  assert.equal(
    instantToWallTime('2026-11-14T20:00:00.000Z', 'Nowhere/Land'),
    '2026-11-14T20:00',
  )
  assert.equal(
    instantToWallTime('2026-03-29T00:59:00.000Z', BERLIN),
    '2026-03-29T01:59',
  )
  assert.equal(
    instantToWallTime('2026-03-29T01:00:00.000Z', BERLIN),
    '2026-03-29T03:00',
  )
  assert.equal(
    instantToWallTime('2026-10-25T00:30:00.000Z', BERLIN),
    '2026-10-25T02:30',
  )
  assert.equal(
    instantToWallTime('2026-10-25T01:30:00.000Z', BERLIN),
    '2026-10-25T02:30',
  )
  assert.equal(instantToWallTime('yesterday', BERLIN), null)
  assert.equal(instantToWallTime('', BERLIN), null)
})

test('the two conversions round-trip for every reading the clock shows once', () => {
  const zones = [
    LONDON,
    BERLIN,
    NEW_YORK,
    'Asia/Kolkata',
    'Pacific/Auckland',
    '+02:00',
    '-05:30',
    'UTC',
  ]
  // Every 15 minutes over the weeks around the four changes.
  for (const zone of zones) {
    for (const [from, to] of [
      [Date.UTC(2026, 2, 6), Date.UTC(2026, 2, 31)],
      [Date.UTC(2026, 9, 23), Date.UTC(2026, 10, 3)],
    ] as const) {
      for (let at = from; at < to; at += 15 * 60 * 1000) {
        const instant = new Date(at).toISOString()
        const wall = instantToWallTime(instant, zone)
        assert.ok(wall, `${zone} ${instant}`)
        const back = wallTimeToInstant(wall, zone)
        assert.ok(back, `${zone} ${wall}`)
        // Where the clock shows a reading twice, the first reading wins; otherwise it is the same instant.
        assert.ok(Date.parse(back) <= at, `${zone} ${instant} ${wall} ${back}`)
        assert.equal(
          instantToWallTime(back, zone),
          wall,
          `${zone} ${instant} ${wall} ${back}`,
        )
      }
    }
  }
})
