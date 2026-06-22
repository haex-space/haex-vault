/**
 * RFC-4180-style CSV utilities.
 *
 * Extracted from the password importers (bitwarden + lastpass) which both
 * shipped a byte-identical `parseCSVLine` plus a thin wrapper around it.
 * Centralising the parsing turns it into a pure function unit-testable
 * outside any Vue/Tauri setup.
 *
 * The parser handles:
 *   - quoted fields containing commas
 *   - escaped quotes inside quoted fields (`""` → `"`)
 *   - bare unquoted fields
 *
 * It deliberately stays small (no streaming, no chunked input, no schema
 * validation) — the call sites parse already-loaded exports of well-known
 * password managers, not arbitrary user input.
 */

/**
 * Tokenise a single CSV line into its fields. Embedded `""` inside a quoted
 * field yields a literal `"`.
 */
export function parseCSVLine(line: string): string[] {
  const result: string[] = []
  let current = ''
  let inQuotes = false
  for (let i = 0; i < line.length; i++) {
    const char = line[i]!
    const next = line[i + 1]
    if (inQuotes) {
      if (char === '"' && next === '"') {
        current += '"'
        i++
      }
      else if (char === '"') {
        inQuotes = false
      }
      else {
        current += char
      }
    }
    else {
      if (char === '"') {
        inQuotes = true
      }
      else if (char === ',') {
        result.push(current)
        current = ''
      }
      else {
        current += char
      }
    }
  }
  result.push(current)
  return result
}

/**
 * Parse a header-rowed CSV string into typed row records.
 *
 * The first non-empty line is treated as the header; subsequent lines are
 * mapped to objects keyed by header name. Empty lines are skipped.
 *
 * `headerTransform` lets callers normalise header names (e.g. LastPass exports
 * with mixed casing that the caller wants lower-cased to match a typed
 * interface — bitwarden keeps the original case).
 *
 * The result is typed via a generic so callers can assert the row shape they
 * expect from a known provider's export. Returns an empty array for a CSV
 * with no data rows (header-only or empty input).
 */
export function parseCSV<T extends Record<string, string>>(
  csvText: string,
  headerTransform?: (header: string) => string,
): T[] {
  const lines = csvText.split('\n')
  if (lines.length < 2) return []

  const rawHeader = parseCSVLine(lines[0] ?? '')
  const header = headerTransform ? rawHeader.map(headerTransform) : rawHeader

  return lines.slice(1)
    .filter(line => line.trim())
    .map((line) => {
      const values = parseCSVLine(line)
      const row: Record<string, string> = {}
      header.forEach((col, idx) => {
        row[col] = values[idx] ?? ''
      })
      return row as T
    })
}
