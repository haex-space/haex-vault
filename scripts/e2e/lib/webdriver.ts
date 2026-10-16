// A minimal W3C WebDriver client over fetch, for exactly the calls the suite needs. Not a general
// client: it talks to the driver that starts the real application (tauri-driver on Linux, chromedriver
// attached to the running app's web view on Android, spec 043).
const ELEMENT_KEY = 'element-6066-11e4-a52e-4f735466cecf'

export type InvokeResult =
  { ok: true; data: unknown } | { ok: false; error: unknown } | { ended: true }

export interface InvokeOptions {
  /** The call closes the application, so no answer is expected: report `{ ended: true }` instead of failing. */
  expectEnd?: boolean
}

/** The driver answered with an error of its own, such as a script error. */
export class WebDriverError extends Error {
  code: string

  constructor(code: string, message: string) {
    super(`${code}: ${message}`)
    this.name = 'WebDriverError'
    this.code = code
  }
}

/** The session or the application behind it is gone, or the driver cannot be reached. */
export class SessionGoneError extends Error {
  constructor(message: string, options: { retryable?: boolean } = {}) {
    super(message)
    this.name = 'SessionGoneError'
    this.retryable = options.retryable ?? false
  }

  /** Whether the request may succeed after the driver finishes starting its native backend. */
  readonly retryable: boolean
}

const GONE_CODES = new Set([
  'invalid session id',
  'no such window',
  'session not created',
  // chromedriver once the Android app it was attached to has ended (spec 043).
  'disconnected',
])

export class WebDriverClient {
  private base: string
  private id: string | null = null
  private limitMs: number

  constructor(baseUrl: string, options: { callLimitMs?: number } = {}) {
    this.base = baseUrl
    // Longer than the 60 s script timeout the session sets, so the driver's own timeout answers first.
    this.limitMs = options.callLimitMs ?? 90_000
  }

  get sessionId(): string | null {
    return this.id
  }

  private async call(
    method: string,
    path: string,
    body?: unknown,
    limitMs = this.limitMs,
  ): Promise<unknown> {
    let response: Response
    try {
      response = await fetch(`${this.base}${path}`, {
        method,
        headers: { 'content-type': 'application/json' },
        body: body === undefined ? undefined : JSON.stringify(body),
        signal: AbortSignal.timeout(limitMs),
      })
    } catch (error) {
      throw new SessionGoneError(
        `${method} ${path} got no answer: ${(error as Error).message}`,
        { retryable: true },
      )
    }
    const text = await response.text()
    let parsed: { value?: unknown } | undefined
    try {
      parsed = text ? (JSON.parse(text) as { value?: unknown }) : undefined
    } catch {
      parsed = undefined
    }
    if (!response.ok) {
      const value = parsed?.value as
        { error?: string; message?: string } | undefined
      const code = value?.error ?? `http ${response.status}`
      const message = value?.message ?? text.slice(0, 200)
      if (
        response.status >= 500 &&
        (value?.error === undefined || value.error === 'unknown error')
      ) {
        throw new SessionGoneError(
          `${method} ${path} failed with ${response.status}: ${message}`,
        )
      }
      if (GONE_CODES.has(code))
        throw new SessionGoneError(`${method} ${path}: ${code}: ${message}`)
      throw new WebDriverError(code, message)
    }
    return parsed?.value
  }

  private session(path: string): string {
    if (this.id === null) throw new Error('no WebDriver session is open')
    return `/session/${this.id}${path}`
  }

  /**
   * Opens a session: for an application path through tauri-driver, or with the capabilities a
   * platform gives (Android: chromedriver attached to the running app).
   */
  async newSession(
    application: string | Record<string, unknown>,
    options: { scriptTimeoutMs?: number } = {},
  ): Promise<string> {
    const alwaysMatch =
      typeof application === 'string'
        ? { 'tauri:options': { application } }
        : application
    const created = (await this.call('POST', '/session', {
      capabilities: { alwaysMatch },
    })) as { sessionId: string }
    this.id = created.sessionId
    await this.call('POST', this.session('/timeouts'), {
      script: options.scriptTimeoutMs ?? 60_000,
    })
    return this.id
  }

  async deleteSession(): Promise<void> {
    if (this.id === null) return
    const path = this.session('')
    this.id = null
    // A hung application must not hang the teardown.
    await this.call('DELETE', path, undefined, 5000)
  }

  async execute<T = unknown>(script: string, args: unknown[] = []): Promise<T> {
    return (await this.call('POST', this.session('/execute/sync'), {
      script,
      args,
    })) as T
  }

  /** Runs a script through WebDriver's asynchronous-script endpoint and awaits its Promise result. */
  async executeAsync<T = unknown>(
    script: string,
    args: unknown[] = [],
  ): Promise<T> {
    const wrapped = `
      const done = arguments[arguments.length - 1]
      const userArgs = Array.prototype.slice.call(arguments, 0, -1)
      try {
        Promise.resolve((function () { ${script} }).apply(null, userArgs))
          .then((value) => done({ ok: true, value }))
          .catch((error) => done({ ok: false, error: String(error) }))
      } catch (error) {
        done({ ok: false, error: String(error) })
      }
    `
    const result = (await this.call('POST', this.session('/execute/async'), {
      script: wrapped,
      args,
    })) as { ok?: boolean; value?: T; error?: string }
    if (result.ok === false) {
      throw new WebDriverError(
        'javascript error',
        result.error ?? 'asynchronous script failed',
      )
    }
    return result.value as T
  }

  /**
   * Call a backend command from the page. Resolves to an object instead of throwing, so an error of the
   * command is told apart from an application that ended before it could answer.
   */
  async invoke(
    command: string,
    args: unknown = {},
    options: InvokeOptions = {},
  ): Promise<InvokeResult> {
    const script = `
      const done = arguments[arguments.length - 1]
      window.__TAURI_INTERNALS__.invoke(${JSON.stringify(command)}, ${JSON.stringify(args)})
        .then((data) => done({ ok: true, data }))
        .catch((error) => done({ ok: false, error }))
    `
    try {
      return (await this.call('POST', this.session('/execute/async'), {
        script,
        args: [],
      })) as InvokeResult
    } catch (error) {
      if (options.expectEnd === true && error instanceof SessionGoneError)
        return { ended: true }
      throw new Error(
        `backend call ${command} failed: ${(error as Error).message}`,
        { cause: error },
      )
    }
  }

  async findElements(using: string, value: string): Promise<string[]> {
    const found = (await this.call('POST', this.session('/elements'), {
      using,
      value,
    })) as Record<string, string>[]
    return found.map((element) => element[ELEMENT_KEY])
  }

  /** Switches the session into an iframe (its element id) or back to the top document (`null`). */
  async switchToFrame(element: string | null): Promise<void> {
    await this.call('POST', this.session('/frame'), {
      id: element === null ? null : { [ELEMENT_KEY]: element },
    })
  }

  /** A left click of the mouse at a point of the viewport: the page's hit test decides which element
   * receives it, as for a user's click. */
  async clickAt(x: number, y: number): Promise<void> {
    await this.call('POST', this.session('/actions'), {
      actions: [
        {
          type: 'pointer',
          id: 'mouse',
          parameters: { pointerType: 'mouse' },
          actions: [
            { type: 'pointerMove', duration: 0, x, y, origin: 'viewport' },
            { type: 'pointerDown', button: 0 },
            { type: 'pause', duration: 80 },
            { type: 'pointerUp', button: 0 },
          ],
        },
      ],
    })
    await this.call('DELETE', this.session('/actions'))
  }

  async click(element: string): Promise<void> {
    await this.call('POST', this.session(`/element/${element}/click`), {})
  }

  /** The element id currently holding focus — for a key (e.g. Enter) meant for wherever a prior
   * key (e.g. ArrowDown) moved focus to, not back to the element that sent that prior key. */
  async activeElement(): Promise<string> {
    const found = (await this.call(
      'GET',
      this.session('/element/active'),
    )) as Record<string, string>
    return found[ELEMENT_KEY]!
  }

  async sendKeys(element: string, text: string): Promise<void> {
    await this.call('POST', this.session(`/element/${element}/value`), { text })
  }

  /**
   * The legacy "is element displayed" endpoint. Confirmed against a real WebKitWebDriver session
   * (2026-09-22): it answers `200 { value: true }` for a genuinely displayed element, so a scenario can
   * tell apart two elements that share a hook when only one is on screen (contracts/test-hooks.md).
   */
  async isDisplayed(element: string): Promise<boolean> {
    return (
      (await this.call(
        'GET',
        this.session(`/element/${element}/displayed`),
      )) === true
    )
  }

  async screenshot(callLimitMs = this.limitMs): Promise<Buffer> {
    return Buffer.from(
      (await this.call(
        'GET',
        this.session('/screenshot'),
        undefined,
        callLimitMs,
      )) as string,
      'base64',
    )
  }

  async closeWindow(): Promise<void> {
    await this.call('DELETE', this.session('/window'))
  }

  async navigate(url: string): Promise<void> {
    await this.call('POST', this.session('/url'), { url })
  }
}
