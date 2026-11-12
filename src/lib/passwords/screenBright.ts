// The screen while a code is shown large (spec 092, FR-008), as a controller with no Vue and no
// window in it, so `scripts/check-passwords-screen-bright.ts` runs it in Node. Every change of the
// screen runs in turn and decides what to do when its turn comes, from what is wanted and visible
// at that moment: an enable that a close has overtaken does nothing, and a wake lock that arrives
// after the close is released at once.

/** The part of a `WakeLockSentinel` the controller uses. */
export type Lock = {
  release: () => Promise<void>
  addEventListener: (type: 'release', listener: () => void) => void
}

export type ScreenParts = {
  /** Full brightness and keep-screen-on (the window setting of Android). */
  setNativeAsync: (on: boolean) => Promise<void>
  /** The webview's Screen Wake Lock, or `null` where it has none. */
  requestLockAsync: () => Promise<Lock | null>
  isVisible: () => boolean
}

export function createScreenController(parts: ScreenParts) {
  let wanted = false
  let lock: Lock | null = null
  let queue: Promise<void> = Promise.resolve()

  const shouldBeOn = () => wanted && parts.isVisible()

  async function stepAsync() {
    const on = shouldBeOn()
    await parts.setNativeAsync(on).catch(() => undefined)
    try {
      if (on) {
        if (lock) return
        const fresh = await parts.requestLockAsync()
        if (fresh && !shouldBeOn()) {
          await fresh.release()
        } else if (fresh) {
          // The system takes the lock back when the window hides: a later turn asks again.
          fresh.addEventListener('release', () => {
            if (lock === fresh) lock = null
          })
          lock = fresh
        }
      } else if (lock) {
        const held = lock
        lock = null
        await held.release()
      }
    } catch {
      // Not allowed or not available: the code is still shown large.
      lock = null
    }
  }

  function syncAsync(): Promise<void> {
    queue = queue.then(stepAsync)
    return queue
  }

  return {
    get wanted() {
      return wanted
    },
    /** The view wants the screen bright and on. */
    onAsync(): Promise<void> {
      wanted = true
      return syncAsync()
    },
    /** The view gives the screen back. */
    offAsync(): Promise<void> {
      wanted = false
      return syncAsync()
    },
    /** The window was hidden or shown. */
    visibilityChangedAsync(): Promise<void> {
      return wanted ? syncAsync() : Promise.resolve()
    },
  }
}
