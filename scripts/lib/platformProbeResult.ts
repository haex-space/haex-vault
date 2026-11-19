/** Judging the one result line of the platform probe (spec 044, T007). */

/** `platform_probe::RESULT_MARKER` in Rust. */
export const RESULT_MARKER = 'HAEX_PROBE_RESULT'

/** T007's desktop runner limit; the probe itself gives up after 90 s. */
export const PROBE_TIMEOUT_MS = 120_000

export interface ProbeVerdict {
  ok: boolean
  message: string
}

/** The last result line in `output`, parsed, or `undefined`. */
export function findResult(output: string): { ok?: unknown } | undefined {
  const lines = output
    .split(/\r?\n/)
    .filter((line) => line.includes(RESULT_MARKER))
  const last = lines.at(-1)
  if (last === undefined) return undefined
  const json = last
    .slice(last.indexOf(RESULT_MARKER) + RESULT_MARKER.length)
    .trim()
  try {
    return JSON.parse(json) as { ok?: unknown }
  } catch {
    return undefined
  }
}

/** Passed only with a result line saying `ok: true` and an exit code of 0 (when one is known). */
export function judgeProbe(
  output: string,
  exitCode: number | null,
): ProbeVerdict {
  const result = findResult(output)
  if (result === undefined) {
    return {
      ok: false,
      message: `platform probe: no ${RESULT_MARKER} line (exit ${exitCode})`,
    }
  }
  if (result.ok !== true) {
    return {
      ok: false,
      message: `platform probe failed: ${JSON.stringify(result)}`,
    }
  }
  if (exitCode !== null && exitCode !== 0) {
    return {
      ok: false,
      message: `platform probe reported ok but exited with ${exitCode}`,
    }
  }
  return { ok: true, message: 'platform probe passed' }
}
