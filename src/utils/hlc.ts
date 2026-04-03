/**
 * Compares two HLC timestamp strings numerically.
 * Format: "<u64_ntp_nanoseconds>/<node_id_hex>"
 */
export function compareHlc(a: string, b: string): number {
  const parse = (s: string): [bigint, string] => {
    const slashIndex = s.indexOf('/')
    if (slashIndex === -1) return [BigInt(s || '0'), '']
    return [BigInt(s.slice(0, slashIndex) || '0'), s.slice(slashIndex + 1)]
  }
  const [aTime, aNode] = parse(a)
  const [bTime, bNode] = parse(b)
  if (aTime !== bTime) return aTime > bTime ? 1 : -1
  if (aNode < bNode) return -1
  if (aNode > bNode) return 1
  return 0
}

/** Returns true if `a` is strictly newer than `b`. */
export function hlcIsNewer(a: string, b: string): boolean {
  return compareHlc(a, b) > 0
}
