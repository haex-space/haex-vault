import { describe, expect, it } from 'vitest'
import { parseCSV, parseCSVLine } from '~/utils/csv'

describe('parseCSVLine', () => {
  it('splits a simple unquoted line', () => {
    expect(parseCSVLine('a,b,c')).toEqual(['a', 'b', 'c'])
  })

  it('preserves empty trailing fields', () => {
    expect(parseCSVLine('a,b,')).toEqual(['a', 'b', ''])
  })

  it('respects quotes around fields with commas', () => {
    expect(parseCSVLine('a,"b,c",d')).toEqual(['a', 'b,c', 'd'])
  })

  it('handles escaped quotes inside a quoted field', () => {
    // CSV `""` inside a quoted field yields a literal `"`.
    expect(parseCSVLine('a,"he said ""hi""",b')).toEqual([
      'a',
      'he said "hi"',
      'b',
    ])
  })

  it('keeps a single bare quote character at the end of a quoted field', () => {
    expect(parseCSVLine('a,"b"')).toEqual(['a', 'b'])
  })

  it('returns an empty single field for an empty line', () => {
    expect(parseCSVLine('')).toEqual([''])
  })
})

interface ProviderRow extends Record<string, string> {
  name: string
  url: string
}

describe('parseCSV', () => {
  it('returns [] for empty input', () => {
    expect(parseCSV('')).toEqual([])
  })

  it('returns [] for a header-only file', () => {
    expect(parseCSV('name,url')).toEqual([])
  })

  it('maps rows to objects keyed by header names', () => {
    const csv = 'name,url\nGitHub,https://github.com\nGitLab,https://gitlab.com'
    expect(parseCSV<ProviderRow>(csv)).toEqual([
      { name: 'GitHub', url: 'https://github.com' },
      { name: 'GitLab', url: 'https://gitlab.com' },
    ])
  })

  it('skips blank lines between data rows', () => {
    const csv = 'name,url\nA,a\n\nB,b\n'
    expect(parseCSV<ProviderRow>(csv)).toEqual([
      { name: 'A', url: 'a' },
      { name: 'B', url: 'b' },
    ])
  })

  it('applies the header transform to normalise header names', () => {
    // LastPass exports use mixed casing; the importer wants lowercase keys to
    // line up with its typed row interface.
    const csv = 'Name,URL\nA,a'
    expect(parseCSV<ProviderRow>(csv, h => h.toLowerCase())).toEqual([
      { name: 'A', url: 'a' },
    ])
  })

  it('fills missing trailing fields with empty strings', () => {
    // Row shorter than header — a real-world Bitwarden quirk on the last row.
    const csv = 'a,b,c\n1,2'
    expect(parseCSV(csv)).toEqual([{ a: '1', b: '2', c: '' }])
  })

  it('preserves embedded commas inside quoted fields end-to-end', () => {
    const csv = 'name,note\nFoo,"hello, world"'
    expect(parseCSV(csv)).toEqual([{ name: 'Foo', note: 'hello, world' }])
  })
})
