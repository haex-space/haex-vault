// The camera of the link sheet's scanner on a desktop (spec 085, research R4 to R6): one session
// that opens the camera, looks at a frame about every 100 ms, hands what a frame decodes to the
// caller and gives the camera back on every way out. No Vue and no DOM beyond what the dependencies
// hand in, so `scripts/check-camera-scan.ts` can drive it with a fake camera and a fake clock.

/** What went wrong, as far as the person can act on it. */
export type CameraErrorKind =
  'noCamera' | 'busy' | 'denied' | 'unsupported' | 'failed'

export class CameraError extends Error {
  kind: CameraErrorKind

  constructor(kind: CameraErrorKind, cause?: unknown) {
    super(`camera: ${kind}`, { cause })
    this.kind = kind
  }
}

/** Maps what `getUserMedia` rejects with to the case a person can act on. */
export function cameraErrorKind(error: unknown): CameraErrorKind {
  const name = (error as { name?: string } | null)?.name
  switch (name) {
    case 'NotFoundError':
    case 'DevicesNotFoundError':
      return 'noCamera'
    case 'NotReadableError':
    case 'TrackStartError':
    case 'AbortError':
      return 'busy'
    case 'NotAllowedError':
    case 'SecurityError':
    case 'PermissionDeniedError':
      return 'denied'
    default:
      return 'failed'
  }
}

export type TrackLike = {
  stop(): void
  getSettings?(): { deviceId?: string }
  addEventListener?(
    type: 'ended',
    listener: () => void,
    options?: { once?: boolean },
  ): void
}
export type StreamLike = { getTracks(): TrackLike[] }
export type VideoLike = {
  srcObject: unknown
  play(): Promise<void>
}
/** One frame as pixels, ready for the decoder. */
export type Frame = { data: Uint8ClampedArray; width: number; height: number }
export type CameraInfo = { deviceId: string; label: string }

export type CameraDeps = {
  /** Opens the host's camera gate; the permission request is answered only while it is open. */
  begin(): Promise<void>
  /** Closes it again; safe at any time. */
  end(): Promise<void>
  /** Whether the webview has a camera API at all. */
  hasCameraApi(): boolean
  getUserMedia(constraints: unknown): Promise<StreamLike>
  cameras(): Promise<CameraInfo[]>
  /** The current picture of `video`, scaled down, or `null` while there is none yet. */
  grab(video: VideoLike): Frame | null
  /** The text a frame holds, or `null`. */
  decode(frame: Frame): string | null
  /** Calls `tick` every `ms` until the returned function is called. */
  every(ms: number, tick: () => void): () => void
}

/** How often a frame is looked at. */
export const FRAME_INTERVAL_MS = 100

// The permission gate belongs to the main window, not this component. Keep open/close commands
// ordered even if one preview is dismissed while its permission request is still resolving and a
// new preview is opened immediately afterwards.
let gateTransition: Promise<void> = Promise.resolve()

function transitionGate(action: () => Promise<void>): Promise<void> {
  const next = gateTransition.then(action)
  gateTransition = next.catch(() => {})
  return next
}

export type CameraScan = {
  /**
   * Opens the camera on `video` (`deviceId` picks one, else the back camera is preferred) and
   * calls `onText` for what frames decode to. `onText` answers `true` when the text was taken:
   * the camera is then released. Rejects with a [`CameraError`]; the camera is released then too.
   */
  start(
    video: VideoLike,
    onText: (text: string) => Promise<boolean>,
    deviceId?: string,
    onError?: (error: CameraError) => void,
  ): Promise<void>
  /** Gives the camera back; idempotent. */
  stop(): Promise<void>
  readonly running: boolean
  readonly activeDeviceId: string | null
}

export function createCameraScan(deps: CameraDeps): CameraScan {
  let stream: StreamLike | null = null
  let video: VideoLike | null = null
  let stopTicking: (() => void) | null = null
  let running = false
  let generation = 0

  function report(
    error: CameraError,
    session: number,
    onError?: (error: CameraError) => void,
  ) {
    if (session !== generation) return
    void stop().then(
      () => onError?.(error),
      () => onError?.(error),
    )
  }

  async function stop() {
    generation += 1
    stopTicking?.()
    stopTicking = null
    for (const track of stream?.getTracks() ?? []) track.stop()
    stream = null
    if (video) video.srcObject = null
    video = null
    running = false
    await transitionGate(deps.end)
  }

  async function open(deviceId: string | undefined): Promise<StreamLike> {
    const preferred = deviceId
      ? { video: { deviceId: { exact: deviceId } } }
      : { video: { facingMode: 'environment' } }
    try {
      return await deps.getUserMedia(preferred)
    } catch (error) {
      // A camera that cannot do what was asked (the back camera of a laptop) is not "no camera".
      if ((error as { name?: string } | null)?.name !== 'OverconstrainedError')
        throw error
      return await deps.getUserMedia({ video: true })
    }
  }

  async function start(
    target: VideoLike,
    onText: (text: string) => Promise<boolean>,
    deviceId?: string,
    onError?: (error: CameraError) => void,
  ) {
    const session = ++generation
    let opening: StreamLike | null = null
    const isCurrent = () => session === generation
    try {
      await transitionGate(deps.begin)
      if (!isCurrent()) return
      if (!deps.hasCameraApi()) throw new CameraError('unsupported')
      opening = await open(deviceId)
      // Closing the preview can happen while the native permission prompt is open. If it resolves
      // afterwards, stop that stream here instead of reviving an unmounted scanner.
      if (!isCurrent()) {
        for (const track of opening.getTracks()) track.stop()
        return
      }
      stream = opening
      opening = null
      video = target
      target.srcObject = stream
      for (const track of stream.getTracks()) {
        track.addEventListener?.(
          'ended',
          () => report(new CameraError('failed'), session, onError),
          { once: true },
        )
      }
      await target.play()
      if (!isCurrent()) return
      running = true
      let reading = false
      stopTicking = deps.every(FRAME_INTERVAL_MS, () => {
        if (!isCurrent()) return
        const current = video
        if (!current || reading) return
        let text: string | null
        try {
          const frame = deps.grab(current)
          text = frame ? deps.decode(frame) : null
        } catch (error) {
          report(new CameraError('failed', error), session, onError)
          return
        }
        if (text === null) return
        // One text at a time: the backend answers while later frames keep coming.
        reading = true
        void onText(text)
          .then((taken) => (taken && isCurrent() ? stop() : undefined))
          .catch(() => undefined)
          .finally(() => {
            reading = false
          })
      })
    } catch (error) {
      for (const track of opening?.getTracks() ?? []) track.stop()
      if (!isCurrent()) return
      await stop()
      throw error instanceof CameraError
        ? error
        : new CameraError(cameraErrorKind(error), error)
    }
  }

  return {
    start,
    stop,
    get running() {
      return running
    },
    get activeDeviceId() {
      return (
        stream
          ?.getTracks()
          .find((track) => track.getSettings)
          ?.getSettings?.().deviceId ?? null
      )
    },
  }
}
