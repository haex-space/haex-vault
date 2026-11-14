// Checks which dialog a file shared to holzi opens (spec 092, FR-037).
// Run: node --test scripts/check-wallet-shared.ts
import assert from 'node:assert/strict'
import { test } from 'node:test'
import { routeShared, type SharedItem } from '../src/lib/wallet/sharedRoute.ts'

const file = (path: string, kind: SharedItem['kind']): SharedItem => ({
  path,
  kind,
})

test('a pass file goes to the import, a PDF and a picture to the capture', () => {
  const routed = routeShared([
    file('/c/a.pkpass', 'pkpass'),
    file('/c/b.pdf', 'pdf'),
    file('/c/c.png', 'image'),
  ])
  assert.deepEqual(routed.imports, ['/c/a.pkpass'])
  assert.deepEqual(routed.captures, [
    { file: '/c/b.pdf', kind: 'pdf' },
    { file: '/c/c.png', kind: 'image' },
  ])
})

test('the order of the files is kept within each queue', () => {
  const routed = routeShared([
    file('/c/1.pkpass', 'pkpass'),
    file('/c/2.png', 'image'),
    file('/c/3.pkpass', 'pkpass'),
    file('/c/4.png', 'image'),
  ])
  assert.deepEqual(routed.imports, ['/c/1.pkpass', '/c/3.pkpass'])
  assert.deepEqual(
    routed.captures.map((c) => c.file),
    ['/c/2.png', '/c/4.png'],
  )
})

test('nothing shared routes to nothing', () => {
  assert.deepEqual(routeShared([]), { imports: [], captures: [] })
})
