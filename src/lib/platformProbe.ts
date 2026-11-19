/**
 * The window's half of the CI's platform probe (spec 044, T007, research R4). Rust sets
 * `window.__HAEX_PROBE__` only in builds with the feature `platform-probe` and only when the run
 * asked for it, so in every other window this does nothing.
 */

export interface ProbeStep {
  name: string
  ok: boolean
  detail?: string
}

export interface ProbeReport {
  steps: ProbeStep[]
  userAgent: string
}

/** The part of an `HTMLImageElement` the diagnosis uses. */
interface ProbeImage {
  onload: (() => void) | null
  onerror: (() => void) | null
  src: string
}

const IMAGE_PROBE_TIMEOUT_MS = 2_000

/** The answer of holzi's health route (`platform_probe::HEALTH_BODY`). */
export const HEALTH_BODY = 'ok'

/** What the page knows about itself; it explains a blocked fetch (policy or network access). */
export interface ProbeContext {
  origin: string
  secure: boolean
  /** Content-Security-Policy violations seen so far, as `directive blocked-uri`. */
  violations: () => string[]
}

/**
 * Checks that the web view can fetch from the loopback server. A fetch the web view blocks (policy
 * or Local/Private Network Access) rejects; its message and any policy violations become the
 * step's detail.
 */
export async function runPlatformProbe(
  port: number,
  fetchFn: typeof fetch,
  userAgent: string,
  context?: ProbeContext,
): Promise<ProbeReport> {
  const steps: ProbeStep[] = [{ name: 'webview', ok: true }]
  if (context) {
    steps.push({
      name: 'context',
      ok: true,
      detail: `origin ${context.origin}, secure context ${context.secure}`,
    })
  }
  const blocked = () => {
    const seen = context?.violations() ?? []
    return seen.length ? `; policy violations: ${seen.join(', ')}` : ''
  }
  try {
    const response = await fetchFn(`http://127.0.0.1:${port}/__probe`, {
      cache: 'no-store',
    })
    const body = await response.text()
    const ok = response.ok && body === HEALTH_BODY
    steps.push({
      name: 'loopback-fetch',
      ok,
      detail: ok
        ? undefined
        : `status ${response.status}, body ${JSON.stringify(body)}${blocked()}`,
    })
  } catch (error) {
    steps.push({
      name: 'loopback-fetch',
      ok: false,
      detail: `${String(error)}${blocked()}`,
    })
  }
  if (!steps.at(-1)?.ok) {
    steps.push({
      name: 'variants',
      ok: true,
      detail: await probeVariants(port, fetchFn),
    })
  }
  return { steps, userAgent }
}

/**
 * Other ways to reach the loopback server, for diagnosis only (they never fail the probe): the
 * server lists what reached it, so a variant that arrives narrows down what the web view blocks.
 */
async function probeVariants(
  port: number,
  fetchFn: typeof fetch,
): Promise<string> {
  const results: string[] = []
  const attempt = async (name: string, run: () => Promise<unknown>) => {
    try {
      await run()
      results.push(`${name}: resolved`)
    } catch (error) {
      results.push(`${name}: ${String(error)}`)
    }
  }
  await attempt('localhost', () =>
    fetchFn(`http://localhost:${port}/__probe?v=localhost`, {
      cache: 'no-store',
    }),
  )
  await attempt('no-cors', () =>
    fetchFn(`http://127.0.0.1:${port}/__probe?v=no-cors`, {
      cache: 'no-store',
      mode: 'no-cors',
    }),
  )
  // `Image` exists in the web view only; the Node checks run without a DOM.
  const ImageCtor = (globalThis as { Image?: new () => ProbeImage }).Image
  if (ImageCtor) {
    await attempt(
      'img',
      () =>
        new Promise<void>((resolve, reject) => {
          const image = new ImageCtor()
          const finish = (error?: Error) => {
            clearTimeout(timeout)
            image.onload = null
            image.onerror = null
            if (error) reject(error)
            else resolve()
          }
          const timeout = setTimeout(
            () => finish(new Error('image timeout')),
            IMAGE_PROBE_TIMEOUT_MS,
          )
          image.onload = () => finish()
          image.onerror = () => finish(new Error('image error'))
          image.src = `http://127.0.0.1:${port}/__probe?v=img`
        }),
    )
  }
  return results.join('; ')
}
