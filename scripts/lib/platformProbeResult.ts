/** Judging the one result line of the platform probe (spec 044, T007). */

/** `platform_probe::RESULT_MARKER` in Rust. */
export const RESULT_MARKER = 'HOLZI_PROBE_RESULT'

/** T007's desktop runner limit; the probe itself gives up after 90 s. */
export const PROBE_TIMEOUT_MS = 120_000

export interface ProbeVerdict {
  ok: boolean
  message: string
}

/** A step that must be in the result with a detail starting so (T090: `all-files`). */
export interface ProbeExpectation {
  step: string
  detail: string
}

type ProbeResult = {
  ok?: unknown
  steps?: { name?: unknown; detail?: unknown }[]
}

/** Reads `--expect <step>=<detail start>`; `undefined` for anything else. */
export function parseExpectation(
  arg: string | undefined,
): ProbeExpectation | undefined {
  const at = arg?.indexOf('=') ?? -1
  if (!arg || at <= 0) return undefined
  return { step: arg.slice(0, at), detail: arg.slice(at + 1) }
}

/** The last result line in `output`, parsed, or `undefined`. */
export function findResult(output: string): ProbeResult | undefined {
  const lines = output
    .split(/\r?\n/)
    .filter((line) => line.includes(RESULT_MARKER))
  const last = lines.at(-1)
  if (last === undefined) return undefined
  const json = last
    .slice(last.indexOf(RESULT_MARKER) + RESULT_MARKER.length)
    .trim()
  try {
    return JSON.parse(json) as ProbeResult
  } catch {
    return undefined
  }
}

/**
 * Passed only with a result line saying `ok: true` and an exit code of 0 (when one is known), and
 * with the step `expect` names, when it names one.
 */
export function judgeProbe(
  output: string,
  exitCode: number | null,
  expect?: ProbeExpectation,
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
  if (expect) {
    const step = result.steps?.find((one) => one.name === expect.step)
    if (
      typeof step?.detail !== 'string' ||
      !step.detail.startsWith(expect.detail)
    ) {
      return {
        ok: false,
        message: `platform probe: step ${expect.step} was ${JSON.stringify(step)}, expected "${expect.detail}"`,
      }
    }
  }
  return { ok: true, message: 'platform probe passed' }
}
