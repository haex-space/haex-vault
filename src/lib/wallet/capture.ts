// The pure parts of capturing a code for a pass (spec 092, FR-034, research R15): which pages of a
// PDF are read, which found codes are picked, and how picked codes join the codes a pass has.
// Pure, so `scripts/check-wallet-capture.ts` runs it without vue.

/** The most pages of a PDF read in one go; a longer one is read in steps of this size. */
export const PAGES_PER_STEP = 20

/** A code found in an image. */
export type FoundCode = { format: string; value: string }

/** A code as an input of a pass (`CodeInput` of the bindings, the parts the capture sets). */
export type CodeEntry = {
  id?: string
  format: string
  value: string
  label?: string
  position: number
}

/**
 * The pages to read from `from` (0-based): up to `PAGES_PER_STEP`, never past the last page, and
 * whether more remain after them.
 */
export function pageStep(
  total: number,
  from: number,
): { pages: number[]; more: boolean } {
  const start = Math.max(0, Math.min(from, total))
  const end = Math.min(total, start + PAGES_PER_STEP)
  return {
    pages: Array.from({ length: end - start }, (_, i) => start + i + 1),
    more: end < total,
  }
}

/** A code found twice (on two pages, or by two reads) is one code. */
export function addFound(
  known: readonly FoundCode[],
  more: readonly FoundCode[],
): FoundCode[] {
  const out = [...known]
  for (const code of more) {
    if (!out.some((c) => c.format === code.format && c.value === code.value)) {
      out.push(code)
    }
  }
  return out
}

/**
 * The codes of a pass after adding picked ones: the stored codes stay as they are (same ids, so
 * their rows and flags are kept), the picked ones are new rows at the end, and a picked code the
 * pass has already is not added twice.
 */
export function withPicked(
  existing: readonly CodeEntry[],
  picked: readonly FoundCode[],
): CodeEntry[] {
  const out: CodeEntry[] = existing.map((code) => ({ ...code }))
  let position = out.reduce((top, code) => Math.max(top, code.position), -1) + 1
  for (const code of picked) {
    if (out.some((c) => c.format === code.format && c.value === code.value)) {
      continue
    }
    out.push({ format: code.format, value: code.value, position })
    position += 1
  }
  return out
}

/** The codes of a new pass from the picked ones, in the order found. */
export function newPassCodes(picked: readonly FoundCode[]): CodeEntry[] {
  return withPicked([], picked)
}
