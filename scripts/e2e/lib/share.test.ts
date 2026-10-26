import assert from 'node:assert/strict'
import { test } from 'node:test'
import { cardBytesOf, cardTextOf } from './share.ts'

// A share and the card inside it, both made by the Rust code (`contacts::share::envelope`).
const LINK =
  'haex-vault://contact/AQNCZW4AAQNCZW4AAAAAAQFCAAAAAQVlbWFpbAR3b3JrB2JAZS5vcmcBhwIAxQGYnAt2y1Y5cf3JvvMewGw1YPMknW7p5dg8V2JVluBfb4DwnccGAwEAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAGRkZGRkZGRkZGRkZGRkZGRkZGRkZGRkZGRkZGRkZGRkARpodHRwczovL3JlbGF5LmV4YW1wbGUub3JnLwEXd3NzOi8vcmVsYXkuZXhhbXBsZS5vcmcLQmVuIEtyw7xnZXIBBWVtYWlsBHdvcmsPYmVuQGV4YW1wbGUub3JnV5ilUyTb59Nqurt4oIUEL1GxEBBI6koz26jF2rA0rQq-32aKLjEoqeV3lPojco2Vgfh1FeQPgw7xiaMOfaUE8w'
const CARD =
  'holzi:card:AMUBmJwLdstWOXH9yb7zHsBsNWDzJJ1u6eXYPFdiVZbgX2-A8J3HBgMBAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABkZGRkZGRkZGRkZGRkZGRkZGRkZGRkZGRkZGRkZGRkZAEaaHR0cHM6Ly9yZWxheS5leGFtcGxlLm9yZy8BF3dzczovL3JlbGF5LmV4YW1wbGUub3JnC0JlbiBLcsO8Z2VyAQVlbWFpbAR3b3JrD2JlbkBleGFtcGxlLm9yZ9gQFQKRjIeLfWIUFqcBnXj1p7ybZceKl1lQiCd96-Pt0CQRD59KCeaiCZyeLX_NIjvdI7jiTsu6pt-pVs0vOS4'

// The two were signed one after the other, so the signatures differ (Schnorr signs with a fresh
// nonce); everything before the 64 signature bytes is the same.
const unsigned = (text: string) => {
  const bytes = Buffer.from(text.slice('holzi:card:'.length), 'base64url')
  return bytes.subarray(0, bytes.length - 64).toString('hex')
}

test('the card inside a share is found behind name, parts, head and details', () => {
  assert.equal(unsigned(cardTextOf(LINK)), unsigned(CARD))
  assert.ok(cardTextOf(LINK).startsWith('holzi:card:'))
})

test('a share without a card has none', () => {
  // version 1, name "A", five empty parts, five empty head fields, no details, no card.
  const bytes = Buffer.from([1, 1, 0x41, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0])
  const link = `haex-vault://contact/${bytes.toString('base64url')}`
  assert.equal(cardBytesOf(link), null)
  assert.throws(() => cardTextOf(link))
})

test('a text that is no share is refused', () => {
  assert.throws(() => cardBytesOf('holzi:card:abc'))
})
