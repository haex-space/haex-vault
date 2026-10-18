// Part of `pnpm check:passwords` (spec 034-password-manager, FR-001, FR-008, FR-021): the small
// helpers of the window (src/lib/passwords/format.ts).
import assert from 'node:assert/strict'
import { test } from 'node:test'

import {
  displayTitle,
  fileKind,
  formatFileSize,
  imageMime,
  isExpired,
  localDay,
  relativeTime,
  safeFileName,
  openableUrl,
} from '../src/lib/passwords/format.ts'

test('an entry without a title has no display title, a spaced one is trimmed', () => {
  assert.equal(displayTitle(null), null)
  assert.equal(displayTitle(undefined), null)
  assert.equal(displayTitle(''), null)
  assert.equal(displayTitle('   '), null)
  assert.equal(displayTitle('  Mail '), 'Mail')
})

test('an entry is expired the day after its expiry day', () => {
  assert.equal(isExpired('2026-10-01', '2026-10-02'), true)
  assert.equal(isExpired('2026-10-02', '2026-10-02'), false)
  assert.equal(isExpired('2026-10-03', '2026-10-02'), false)
  assert.equal(isExpired(null, '2026-10-02'), false)
  assert.equal(isExpired('soon', '2026-10-02'), false)
})

test('localDay formats the local calendar day', () => {
  assert.equal(localDay(new Date(2026, 9, 2, 23, 59)), '2026-10-02')
  assert.equal(localDay(new Date(2026, 0, 5)), '2026-01-05')
})

test('file sizes read in bytes, KiB, MiB and GiB with two decimals', () => {
  assert.equal(formatFileSize(0), '0 Bytes')
  assert.equal(formatFileSize(1023), '1023 Bytes')
  assert.equal(formatFileSize(1024), '1.00 KiB')
  assert.equal(formatFileSize(1536), '1.50 KiB')
  assert.equal(formatFileSize(25 * 1024 * 1024), '25.00 MiB')
  assert.equal(formatFileSize(3 * 1024 ** 3), '3.00 GiB')
  assert.equal(formatFileSize(-1), '0 Bytes')
  assert.equal(formatFileSize(Number.NaN), '0 Bytes')
})

test('a file name becomes text: separators and control characters are replaced', () => {
  assert.equal(safeFileName('report.pdf'), 'report.pdf')
  assert.equal(safeFileName('../../etc/passwd'), '.._.._etc_passwd')
  assert.equal(safeFileName('a\\b/c'), 'a_b_c')
  assert.equal(safeFileName('line\nbreak\t\u0000'), 'line_break__')
  assert.equal(safeFileName('Übergrößen 🔑.txt'), 'Übergrößen 🔑.txt')
  assert.equal(safeFileName('x'.repeat(300)).length, 255)
})

test('only the five image extensions can be previewed, whatever their case', () => {
  assert.equal(imageMime('photo.PNG'), 'image/png')
  assert.equal(imageMime('a.b.jpeg'), 'image/jpeg')
  assert.equal(imageMime('x.jpg'), 'image/jpeg')
  assert.equal(imageMime('x.gif'), 'image/gif')
  assert.equal(imageMime('x.webp'), 'image/webp')
  assert.equal(imageMime('x.svg'), null)
  assert.equal(imageMime('archive.zip'), null)
  assert.equal(imageMime('noextension'), null)
})

test('relativeTime names a past time in the largest fitting unit, in the language asked', () => {
  const now = new Date('2026-10-04T12:00:00Z')
  const ago = (ms: number) => new Date(now.getTime() - ms)
  const minute = 60_000
  const hour = 60 * minute
  const day = 24 * hour
  assert.equal(relativeTime(ago(10_000), now, 'de'), 'jetzt')
  assert.equal(relativeTime(ago(5 * minute), now, 'de'), 'vor 5 Minuten')
  assert.equal(relativeTime(ago(3 * hour), now, 'de'), 'vor 3 Stunden')
  assert.equal(relativeTime(ago(3 * day), now, 'de'), 'vor 3 Tagen')
  assert.equal(relativeTime(ago(day), now, 'en'), 'yesterday')
  assert.equal(relativeTime(ago(2 * day), now, 'en'), '2 days ago')
  assert.equal(relativeTime(ago(40 * day), now, 'en'), 'last month')
  assert.equal(relativeTime(ago(800 * day), now, 'en'), '2 years ago')
})

test('relativeTime of a time that is not in the past reads as now', () => {
  const now = new Date('2026-10-04T12:00:00Z')
  assert.equal(relativeTime(new Date(now.getTime() + 5000), now, 'en'), 'now')
})

test('an attachment has a kind by its extension: image, pdf, text or other', () => {
  for (const name of ['a.png', 'b.JPG', 'c.jpeg', 'd.gif', 'e.webp']) {
    assert.equal(fileKind(name), 'image', name)
  }
  assert.equal(fileKind('Vertrag.PDF'), 'pdf')
  for (const name of ['a.txt', 'b.md', 'c.json', 'd.csv', 'e.log']) {
    assert.equal(fileKind(name), 'text', name)
  }
  // SVG can carry script; it is never previewed.
  for (const name of [
    'logo.svg',
    'archiv.zip',
    'ohne-endung',
    '.png.exe',
    'punkt.',
  ]) {
    assert.equal(fileKind(name), 'other', name)
  }
})

test('a stored address opens with its own scheme when that is a known one, else never', () => {
  assert.equal(openableUrl('https://example.com/a'), 'https://example.com/a')
  assert.equal(openableUrl(' http://example.com '), 'http://example.com')
  assert.equal(openableUrl('example.com/login'), 'https://example.com/login')
  assert.equal(openableUrl('localhost:8080'), 'https://localhost:8080')
  assert.equal(openableUrl('FTP://example.com'), 'ftp://example.com')
  assert.equal(openableUrl('ssh://me@host:22'), 'ssh://me@host:22')
  assert.equal(
    openableUrl('file:///home/me/notes.txt'),
    'file:///home/me/notes.txt',
  )
  assert.equal(openableUrl('javascript://%0Aalert(1)'), null)
  assert.equal(openableUrl('javascript:alert(1)'), null)
  assert.equal(openableUrl('data://text/html,x'), null)
  assert.equal(openableUrl('mailto:person@example.com'), null)
  assert.equal(openableUrl('foo://example.com'), null)
  assert.equal(openableUrl('  '), null)
  assert.equal(openableUrl(null), null)
})
