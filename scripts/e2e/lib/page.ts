// The interaction surface a scenario drives: reach a control by its stable hook (contracts/test-hooks.md)
// through the driver's own find/displayed/click/send-keys calls, never by displayed text and never by
// running a lookup script on the page. The "is element displayed" endpoint (used to tell apart two
// elements that share a hook when only one is on screen) was confirmed against a real WebKitWebDriver
// session on 2026-09-22; a real `click()` call was confirmed on the same date to return promptly (tens of
// milliseconds) even when the click ends the application, so it needs no special scheduling either
// (research: pressing Enter did not submit the unlock form; the page a closing invoke() replaces gets no
// reply, which is why `exec`/`invoke` still go through a script).
import type {
  InvokeOptions,
  InvokeResult,
  WebDriverClient,
} from './webdriver.ts'
import { SessionGoneError, WebDriverError } from './webdriver.ts'
import { findByMarker, pidAlive } from './processes.ts'
import type { MarkedProcess } from './processes.ts'

/**
 * Whether `error` is a legitimate way for a click to land once the application is already on its way
 * out: the session itself may already be gone, or - observed for real, under load, where the DOM was
 * torn down before the session was - the element the earlier click found is now a stale reference (or
 * gone outright). Anything else is a real failure and still propagates.
 */
function isGoneMidPress(error: unknown): boolean {
  return (
    error instanceof SessionGoneError ||
    (error instanceof WebDriverError &&
      (error.code === 'stale element reference' ||
        error.code === 'no such element' ||
        error.code === 'element not interactable'))
  )
}

const sleep = (ms: number) =>
  new Promise<void>((resolve) => setTimeout(resolve, ms))

/** A record of something the scenario did, on the runner's one clock (contracts/helpers.md). */
export interface StepRecorder {
  (name: string, detail?: string, device?: string): void
}

/** A value starting with `#`, `.` or `[` is a selector as is; anything else is a `data-testid`. */
export function toSelector(hook: string): string {
  const first = hook[0]
  return first === '#' || first === '.' || first === '['
    ? hook
    : `[data-testid="${hook}"]`
}

/** Every element id found for hook that is genuinely displayed right now (there may be several ids for
 * one hook - contracts/test-hooks.md says at most one is ever on screen). */
async function displayedNow(
  client: WebDriverClient,
  selector: string,
): Promise<string[]> {
  const ids = await client.findElements('css selector', selector)
  const displayed: string[] = []
  for (const id of ids) {
    try {
      if (await client.isDisplayed(id)) displayed.push(id)
    } catch {
      // A stale reference (the page moved on between finding and asking) is not displayed either.
    }
  }
  return displayed
}

/** Polls find+is-displayed until one element for hook is on screen, or fails naming the hook. */
async function findDisplayed(
  client: WebDriverClient,
  hook: string,
  deadlineMs: number,
): Promise<string> {
  const selector = toSelector(hook)
  const end = Date.now() + deadlineMs
  for (;;) {
    const [id] = await displayedNow(client, selector)
    if (id !== undefined) return id
    if (Date.now() >= end) {
      throw new Error(
        `hook "${hook}" (selector ${selector}) was not displayed within ${deadlineMs} ms`,
      )
    }
    await sleep(50)
  }
}

const RETRIED_CLICK_ERRORS = new Set([
  'element not interactable',
  'element click intercepted',
  // The page re-rendered the control between finding and clicking it (seen in CI on a breadcrumb):
  // the click never landed, so finding it again and clicking is safe.
  'stale element reference',
])

function isDetachedClickError(error: unknown): boolean {
  return (
    error instanceof WebDriverError &&
    error.code === 'javascript error' &&
    error.message.includes('el.dispatchEvent')
  )
}

/** Clicks the displayed control and returns the element ID that was activated. */
async function clickDisplayed(
  client: WebDriverClient,
  hook: string,
  deadlineMs = 5000,
  onClickSent?: () => void,
  settle = false,
): Promise<string> {
  const end = Date.now() + deadlineMs
  for (;;) {
    const element = await findDisplayed(
      client,
      hook,
      Math.max(0, end - Date.now()),
    )
    try {
      // Keep fixed desktop chrome such as the dock from intercepting a control that was just
      // revealed below a scroll surface (the WebDriver click otherwise only scrolls it to an edge).
      // With `settle` the script then waits until the control stands still (see `SETTLE_MS`): the
      // driver clicks the centre of the box it measures at that moment, and a control that is still
      // moving (a row above it collapsing, the list settling after a scroll) is not where it was.
      await client.execute(
        `const elements = document.querySelectorAll(arguments[0])
         const settle = arguments[1]
         const limit = arguments[2]
         for (const candidate of elements) {
           const rect = candidate.getBoundingClientRect()
           const style = getComputedStyle(candidate)
           if (style.visibility !== 'hidden' && style.display !== 'none' && rect.width > 0 && rect.height > 0) {
             candidate.scrollIntoView({ block: 'center', inline: 'nearest' })
             if (!settle || document.hidden) return true
             return new Promise((resolve) => {
               const place = () => {
                 const box = candidate.getBoundingClientRect()
                 return box.left + ',' + box.top + ',' + box.width + ',' + box.height
               }
               let last = place()
               let still = 0
               // A page that draws no frames (in the background) must not hold the click up.
               const timer = setTimeout(() => resolve(true), limit)
               const tick = () => {
                 const now = place()
                 still = now === last ? still + 1 : 0
                 last = now
                 if (still >= 2) {
                   clearTimeout(timer)
                   resolve(true)
                 } else requestAnimationFrame(tick)
               }
               requestAnimationFrame(tick)
             })
           }
         }
         return false`,
        [toSelector(hook), settle, SETTLE_MS],
      )
      onClickSent?.()
      await client.click(element)
      return element
    } catch (error) {
      // Not interactable and intercepted are both "not ready yet": a window that was just brought to
      // the front may still be animating over the control (found with scenarios that switch apps).
      if (
        !(error instanceof WebDriverError) ||
        (!RETRIED_CLICK_ERRORS.has(error.code) &&
          !isDetachedClickError(error)) ||
        Date.now() >= end
      ) {
        // Name the control: a bare driver error ("element click intercepted") does not say which one.
        throw error instanceof WebDriverError
          ? new Error(`clicking hook "${hook}" failed: ${error.message}`, {
              cause: error,
            })
          : error
      }
      await sleep(50)
    }
  }
}

/**
 * The longest a click waits for its control to stand still: the box of the control is the same over
 * two animation frames. Short, so a control that never stands still (a spinner) costs this and then
 * gets its click.
 */
const SETTLE_MS = 300

/**
 * Clicks the displayed control found by hook, once it stands still. Polls until the deadline; fails
 * naming the hook.
 */
export async function click(
  client: WebDriverClient,
  hook: string,
  deadlineMs = 5000,
): Promise<void> {
  await clickDisplayed(client, hook, deadlineMs, undefined, true)
}

/** Waits until a stable hook is displayed without activating it. */
export async function waitForDisplayed(
  client: WebDriverClient,
  hook: string,
  deadlineMs = 5000,
): Promise<void> {
  await findDisplayed(client, hook, deadlineMs)
}

/** Types into the displayed control found by hook. Polls until the deadline; fails naming the hook. */
export async function type(
  client: WebDriverClient,
  hook: string,
  text: string,
  deadlineMs = 5000,
): Promise<void> {
  const element = await findDisplayed(client, hook, deadlineMs)
  await client.sendKeys(element, text)
}

/** Sends a key to wherever focus currently is, rather than re-selecting (and so re-focusing) a
 * named hook — for a key meant to land where an earlier key (e.g. Arrow Down in a listbox) moved
 * focus to, such as Enter to commit a highlighted option. */
export async function typeToFocused(
  client: WebDriverClient,
  text: string,
): Promise<void> {
  const element = await client.activeElement()
  await client.sendKeys(element, text)
}

export interface PressOptions {
  /** Click this many times in a row (the lock-twice check). Default 1. */
  times?: number
  step: StepRecorder
}

/**
 * Clicks the displayed control found by hook, `options.times` times in a row. The control must already
 * be displayed; unlike `click`, this does not poll for it, so a scenario relying on it being on screen
 * right now gets a clear failure instead of a silent wait.
 *
 * Resolves with the runner-clock time the first click request was sent: the moment of the press itself,
 * for a deadline measured from it. The element lookups before it are WebDriver round trips that, under
 * load (a reply streaming on a CI runner), took over a second - time a deadline taken before the call
 * would wrongly charge to the application.
 *
 * A click can find the application already ending: the session may disappear after the browser
 * dispatches the lock event but before WebDriver answers, or - seen for real, under load - only the
 * element it found may be stale because the DOM was torn down first. Either quietly stops the
 * remaining clicks, rather than failing the call. A click that fails for any other reason still throws.
 */
export async function press(
  client: WebDriverClient,
  hook: string,
  options: PressOptions,
): Promise<number> {
  const selector = toSelector(hook)
  let [element] = await displayedNow(client, selector)
  if (element === undefined) {
    throw new Error(`hook "${hook}" (selector ${selector}) is not displayed`)
  }
  let pressedAt = Date.now()
  const times = options.times ?? 1
  for (let i = 0; i < times; i++) {
    try {
      if (i === 0)
        element = await clickDisplayed(client, hook, undefined, () => {
          pressedAt = Date.now()
        })
      else await client.click(element)
    } catch (error) {
      if (isGoneMidPress(error)) break
      throw error
    }
  }
  options.step('press', hook)
  return pressedAt
}

/**
 * Resolves with the time from the call to the end of `pid`, and records `process-ended`. Fails at the
 * deadline instead of waiting forever for a process that will not end.
 */
export async function waitForEnd(
  pid: number,
  deadlineMs: number,
  step: StepRecorder,
): Promise<number> {
  return waitUntilGone(() => pidAlive(pid), `process ${pid}`, deadlineMs, step)
}

/** `waitForEnd` for a process only `alive` can see (an app on an Android device, spec 043). */
export async function waitUntilGone(
  alive: () => boolean,
  label: string,
  deadlineMs: number,
  step: StepRecorder,
): Promise<number> {
  const startedAt = Date.now()
  const end = startedAt + deadlineMs
  while (alive()) {
    if (Date.now() >= end) {
      throw new Error(`${label} did not end within ${deadlineMs} ms`)
    }
    await sleep(50)
  }
  const elapsedMs = Date.now() - startedAt
  step('process-ended', `${elapsedMs} ms`)
  return elapsedMs
}

/** The run's marked processes running the application, for the relaunch and lock-twice checks. */
export function markedProcesses(
  marker: string,
  executable: string,
): MarkedProcess[] {
  return findByMarker(marker, executable)
}

/**
 * Runs `script` on the page every `intervalMs` and collects the results, until the session (and so the
 * application) ends. Any other failure of the script still throws.
 */
export async function sampleUntilEnd<T = unknown>(
  client: WebDriverClient,
  script: string,
  intervalMs: number,
): Promise<T[]> {
  const samples: T[] = []
  for (;;) {
    try {
      samples.push(await client.execute<T>(script))
    } catch (error) {
      if (error instanceof SessionGoneError) return samples
      throw error
    }
    await sleep(intervalMs)
  }
}

/** The interaction surface exposed on an instance (contracts/helpers.md). */
export interface Page {
  invoke(
    command: string,
    args?: unknown,
    options?: InvokeOptions,
  ): Promise<InvokeResult>
  click(hook: string, deadlineMs?: number): Promise<void>
  /** Clicks a point of the viewport rather than an element: whatever the page's hit test finds there
   * receives the click, even an element behind another one. */
  clickAt(x: number, y: number): Promise<void>
  waitForDisplayed(hook: string, deadlineMs?: number): Promise<void>
  type(hook: string, text: string, deadlineMs?: number): Promise<void>
  typeToFocused(text: string): Promise<void>
  press(hook: string, options?: { times?: number }): Promise<number>
  closeWindow(): Promise<void>
  navigate(url: string): Promise<void>
  exec<T = unknown>(script: string, args?: unknown[]): Promise<T>
  waitForEnd(deadlineMs: number): Promise<number>
  markedProcesses(): MarkedProcess[]
  sampleUntilEnd<T = unknown>(script: string, intervalMs: number): Promise<T[]>
  /** Runs the following calls inside the displayed iframe found by hook, or back in the top
   * document with `null` (spec 017: extension frames). */
  frame(hook: string | null, deadlineMs?: number): Promise<void>
}

/** The application process as a platform sees it, where it is not a process on this machine. */
export interface PageProcess {
  alive(): boolean
  marked(): MarkedProcess[]
}

export type PageOptions = {
  client: WebDriverClient
  step: StepRecorder
  /** Maps a URL of the desktop app to the same page on this platform (Android: `http://tauri.localhost`). */
  mapUrl?: (url: string) => string
  /** Ends the window on this platform (Android: removes the task); default: the driver's own. */
  closeWindow?: () => Promise<void>
  /**
   * Navigates by replacing the current history entry, as the app's own router does. Android's web
   * view adds an entry for a driver navigation, which `history.back()` would then reach; holzi
   * itself never has one (spec 043, scenario tab-content-isolation).
   */
  replaceOnNavigate?: boolean
} & (
  | {
      /** The application process this page's session started. */
      appPid: number
      marker: string
      /** The application's real, resolved path (what `markedProcesses` matches on). */
      executable: string
    }
  | { process: PageProcess }
)

/** Loads `url` in place of the current history entry and waits until it has loaded. */
async function replaceLocation(
  client: WebDriverClient,
  url: string,
  deadlineMs = 15_000,
): Promise<void> {
  await client.execute('location.replace(arguments[0]); return true', [url])
  const end = Date.now() + deadlineMs
  for (;;) {
    // While the old document goes, a script may fail; the next try reaches the new one.
    const loaded = await client
      .execute<boolean>(
        'return location.href === arguments[0] && document.readyState === "complete"',
        [url],
      )
      .catch(() => false)
    if (loaded) return
    if (Date.now() >= end) {
      throw new Error(`timed out after ${deadlineMs} ms loading ${url}`)
    }
    await new Promise((resolve) => setTimeout(resolve, 50))
  }
}

/** Binds the functions above to one instance's client, process and marker. */
export function createPage(options: PageOptions): Page {
  const { client, step } = options
  const processes: PageProcess & { label: string } =
    'process' in options
      ? { ...options.process, label: 'the application' }
      : {
          alive: () => pidAlive(options.appPid),
          marked: () => markedProcesses(options.marker, options.executable),
          label: `process ${options.appPid}`,
        }
  const mapUrl = options.mapUrl ?? ((url: string) => url)
  return {
    invoke: (command, args, invokeOptions) =>
      client.invoke(command, args, invokeOptions),
    click: (hook, deadlineMs) => click(client, hook, deadlineMs),
    clickAt: (x, y) => client.clickAt(x, y),
    waitForDisplayed: (hook, deadlineMs) =>
      waitForDisplayed(client, hook, deadlineMs),
    type: (hook, text, deadlineMs) => type(client, hook, text, deadlineMs),
    typeToFocused: (text) => typeToFocused(client, text),
    press: (hook, pressOptions) =>
      press(client, hook, { ...pressOptions, step }),
    closeWindow: options.closeWindow ?? (() => client.closeWindow()),
    navigate: (url) =>
      options.replaceOnNavigate === true
        ? replaceLocation(client, mapUrl(url))
        : client.navigate(mapUrl(url)),
    exec: (script, args) => client.execute(script, args),
    waitForEnd: (deadlineMs) =>
      waitUntilGone(processes.alive, processes.label, deadlineMs, step),
    markedProcesses: () => processes.marked(),
    sampleUntilEnd: (script, intervalMs) =>
      sampleUntilEnd(client, script, intervalMs),
    frame: async (hook, deadlineMs = 5000) =>
      client.switchToFrame(
        hook === null ? null : await findDisplayed(client, hook, deadlineMs),
      ),
  }
}
