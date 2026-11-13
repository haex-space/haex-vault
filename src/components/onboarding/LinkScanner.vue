<script setup lang="ts">
/**
 * The camera of the link sheet on a desktop (spec 085, user story 2): a live preview inside the
 * sheet that reads the QR code of the main device. It opens when it is shown and gives the camera
 * back when it closes, the sheet closes, the window is left or an error ends it; nothing of the
 * picture is kept. What a frame holds is handed to `accept`, which says whether it was a link text:
 * only then the scan ends, anything else keeps looking. A camera that cannot be used is a toast
 * with the way out; pasting and typing the code stay possible.
 *
 * Where the webview's camera is no use (WebKitGTK on Linux plays a camera stream as one frozen
 * picture) the backend reads the camera and sends the pictures and the text of a QR code as events;
 * the preview is then an image, everything else is the same (research R4, `link_camera`).
 */
import { Channel, invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { toast } from 'vue-sonner'
import {
  CameraError,
  createCameraScan,
  type CameraDeps,
  type CameraErrorKind,
  type Frame,
} from '~/lib/sync/cameraScan'
import type { FoundCode } from '~/lib/wallet/capture'

interface Props {
  /** Reads a text from a frame; `true` when it was the code of a vault and the scan may end. */
  accept?: (text: string) => Promise<boolean>
  /** Reads the codes of a pass from a frame; `true` when they were taken and the scan may end. */
  acceptCodes?: (codes: FoundCode[]) => Promise<boolean>
}

const props = defineProps<Props>()
const emit = defineEmits<{ close: [] }>()

const { t } = useI18n()
const { scanImageAsync } = usePassCapture()
/** What is read: the code of a vault (QR, a text) or the codes of a pass (every format). */
const codesMode = props.acceptCodes !== undefined
/** Where the words of the hint and the failures are: a pass names other ways out than a link. */
const words = codesMode ? 'wallet.capture.camera' : 'onboarding.link.camera'
const region = ref<HTMLElement | null>(null)
/** Whether the backend reads the camera; `null` until it has been asked. */
const native = ref<boolean | null>(null)
/** The latest picture of the backend's camera, as an object URL of its JPEG bytes. */
const preview = ref('')
let nativeListeners: UnlistenFn[] = []
const video = ref<HTMLVideoElement | null>(null)
const cameras = ref<{ deviceId: string; label: string }[]>([])
const cameraId = ref('')
const cameraOptions = computed(() =>
  cameras.value.map((camera) => ({
    value: camera.deviceId,
    label: camera.label || t('onboarding.link.camera.unnamed'),
  })),
)
const hint = ref(false)
let hintTimer: ReturnType<typeof setTimeout> | undefined

type Decoder = (
  data: Uint8ClampedArray,
  width: number,
  height: number,
) => { data: string } | null
let decoder: Decoder | null = null
const canvas = document.createElement('canvas')
const context = canvas.getContext('2d', { willReadFrequently: true })

/**
 * The longest side of a looked-at frame: enough for a QR code at reading distance, cheap to decode.
 * A ticket's PDF417 or a thin bar code needs more pixels.
 */
const FRAME_SIDE = codesMode ? 1280 : 640

/** What a frame gives: the text of a link code, or the codes of a pass. */
type Found = string | FoundCode[]

/** The codes of the frame just drawn on the canvas; the backend reads them, nothing is kept. */
async function readCodesAsync(): Promise<FoundCode[] | null> {
  const blob = await new Promise<Blob | null>((resolve) =>
    canvas.toBlob(resolve, 'image/jpeg', 0.85),
  )
  if (!blob) return null
  const found = await scanImageAsync(new Uint8Array(await blob.arrayBuffer()))
  return found.length > 0 ? found : null
}

/** Hands what was read to the caller; `true` ends the scan. */
function deliver(found: Found): Promise<boolean> {
  return typeof found === 'string'
    ? (props.accept?.(found) ?? Promise.resolve(false))
    : (props.acceptCodes?.(found) ?? Promise.resolve(false))
}

const deps: CameraDeps<Found> = {
  begin: () => invoke('camera_scan_begin'),
  end: () => invoke('camera_scan_end'),
  hasCameraApi: () =>
    typeof navigator.mediaDevices?.getUserMedia === 'function',
  getUserMedia: (constraints) =>
    navigator.mediaDevices.getUserMedia(constraints as MediaStreamConstraints),
  cameras: async () =>
    (await navigator.mediaDevices.enumerateDevices())
      .filter((device) => device.kind === 'videoinput')
      .map((device) => ({ deviceId: device.deviceId, label: device.label })),
  grab: (source): Frame | null => {
    const el = source as unknown as HTMLVideoElement
    if (!context || el.videoWidth === 0) return null
    const scale = Math.min(
      1,
      FRAME_SIDE / Math.max(el.videoWidth, el.videoHeight),
    )
    canvas.width = Math.round(el.videoWidth * scale)
    canvas.height = Math.round(el.videoHeight * scale)
    context.drawImage(el, 0, 0, canvas.width, canvas.height)
    return context.getImageData(0, 0, canvas.width, canvas.height)
  },
  // `grab` has just drawn the frame on the canvas, which is what the backend gets.
  decode: (frame) =>
    codesMode
      ? readCodesAsync()
      : (decoder?.(frame.data, frame.width, frame.height)?.data ?? null),
  every: (ms, tick) => {
    const id = setInterval(tick, ms)
    return () => clearInterval(id)
  },
}
const scan = createCameraScan<Found>(deps)
let openRequest = 0

/** Which system's words say where the camera is allowed. */
function system(): 'windows' | 'mac' | 'linux' {
  const platform = navigator.platform.toLowerCase()
  if (platform.includes('win')) return 'windows'
  if (platform.includes('mac')) return 'mac'
  return 'linux'
}

function failure(kind: CameraErrorKind): string {
  return kind === 'denied'
    ? t(`${words}.denied.${system()}`)
    : t(`${words}.${kind}`)
}

async function open(deviceId?: string) {
  const request = ++openRequest
  const element = video.value
  if (!element) return
  try {
    if (!codesMode) decoder ??= (await import('jsqr')).default
    if (request !== openRequest) return
    await scan.start(element, deliver, deviceId, (error) => {
      if (request !== openRequest) return
      toast.error(failure(error.kind), { id: 'link-camera' })
      emit('close')
    })
    if (request !== openRequest) {
      await scan.stop()
      return
    }
    // Names of cameras are only there after the first grant.
    cameras.value = await deps.cameras()
    cameraId.value = scan.activeDeviceId ?? cameras.value[0]?.deviceId ?? ''
  } catch (e: unknown) {
    if (request !== openRequest) return
    const kind = e instanceof CameraError ? e.kind : 'failed'
    toast.error(failure(kind), { id: 'link-camera' })
    emit('close')
  }
}

async function onPick(deviceId: string) {
  openRequest += 1
  cameraId.value = deviceId
  await scan.stop()
  await open(deviceId)
}

function onPickValue(value: unknown) {
  if (typeof value === 'string') void onPick(value)
}

const KINDS: CameraErrorKind[] = [
  'noCamera',
  'busy',
  'denied',
  'unsupported',
  'failed',
]

/** The reason of a backend camera error (`CameraUnavailable`) or an event, as a known kind. */
function nativeKind(reason: unknown): CameraErrorKind {
  const word =
    typeof reason === 'string'
      ? reason
      : (reason as { reason?: string } | null)?.reason
  return KINDS.find((kind) => kind === word) ?? 'failed'
}

/** Shows a picture of the backend's camera; the one before it is let go. */
function showFrame(bytes: ArrayBuffer | number[]) {
  const data = bytes instanceof ArrayBuffer ? bytes : new Uint8Array(bytes)
  const before = preview.value
  preview.value = URL.createObjectURL(new Blob([data], { type: 'image/jpeg' }))
  if (before) URL.revokeObjectURL(before)
}

async function stopNative() {
  for (const stop of nativeListeners) stop()
  nativeListeners = []
  if (preview.value) URL.revokeObjectURL(preview.value)
  preview.value = ''
  await invoke('link_camera_stop').catch(() => undefined)
}

function nativeFailed(reason: unknown) {
  toast.error(failure(nativeKind(reason)), { id: 'link-camera' })
  void stopNative()
  emit('close')
}

async function openNative() {
  const request = ++openRequest
  try {
    nativeListeners = [
      codesMode
        ? await listen<FoundCode[]>('link-camera-codes', async (event) => {
            if (await deliver(event.payload)) await stopNative()
          })
        : await listen<string>('link-camera-text', async (event) => {
            if (await deliver(event.payload)) await stopNative()
          }),
      await listen<string>('link-camera-error', (event) =>
        nativeFailed(event.payload),
      ),
    ]
    // Every picture comes as raw bytes through this channel, not as text in an event.
    const frames = new Channel<ArrayBuffer | number[]>()
    frames.onmessage = showFrame
    await invoke('link_camera_start', { onFrame: frames, codes: codesMode })
    if (request !== openRequest) await stopNative()
  } catch (e: unknown) {
    if (request === openRequest) nativeFailed(e)
  }
}

/** Gives the camera back, whoever reads it. */
async function stopCamera() {
  openRequest += 1
  await Promise.all([scan.stop(), stopNative()])
}

/** The picture, the sheet or the window went away: the camera goes back. */
async function release() {
  await stopCamera()
  emit('close')
}

async function begin() {
  // Asked first, so a release while the questions below are open is seen after them.
  const request = ++openRequest
  native.value = await invoke<boolean>('link_camera_native').catch(() => false)
  // The video element exists only where the webview reads the camera.
  await nextTick()
  if (request !== openRequest) return
  if (native.value) await openNative()
  else await open()
}

function onVisibility() {
  if (document.visibilityState === 'hidden') void release()
}

onMounted(() => {
  // Keyboard users land in the preview, on the button that closes it.
  region.value
    ?.querySelector<HTMLElement>('[data-testid="link-scanner-close"]')
    ?.focus()
  void begin()
  hintTimer = setTimeout(() => (hint.value = true), 15_000)
  window.addEventListener('blur', release)
  document.addEventListener('visibilitychange', onVisibility)
})
onBeforeUnmount(() => {
  openRequest += 1
  clearTimeout(hintTimer)
  window.removeEventListener('blur', release)
  document.removeEventListener('visibilitychange', onVisibility)
  void stopCamera()
})
</script>

<template>
  <section
    ref="region"
    class="space-y-2 rounded-md border p-3"
    data-testid="link-scanner"
    :aria-label="t('onboarding.link.camera.region')"
  >
    <div class="relative">
      <img
        v-if="native && preview"
        :src="preview"
        class="aspect-video w-full rounded bg-muted object-cover"
        :alt="t('onboarding.link.camera.preview')"
        data-testid="link-scanner-image"
      />
      <div
        v-else-if="native"
        class="aspect-video w-full rounded bg-muted"
        role="img"
        :aria-label="t('onboarding.link.camera.preview')"
      />
      <video
        v-else
        ref="video"
        class="aspect-video w-full rounded bg-muted"
        muted
        playsinline
        :aria-label="t('onboarding.link.camera.preview')"
      />
      <UiButton
        type="button"
        variant="secondary"
        size="icon"
        class="absolute top-2 right-2 size-8 rounded-full"
        :aria-label="t('onboarding.link.camera.close')"
        data-testid="link-scanner-close"
        @click="release"
      >
        <Icon name="lucide:x" class="size-4" />
      </UiButton>
    </div>
    <p
      v-if="hint"
      class="text-sm text-muted-foreground"
      role="status"
      data-testid="link-scanner-hint"
    >
      {{ t(`${words}.hint`) }}
    </p>
    <div v-if="cameras.length > 1" class="flex items-center gap-2">
      <UiSelect
        :model-value="cameraId || cameras[0]?.deviceId || ''"
        :options="cameraOptions"
        :aria-label="t('onboarding.link.camera.pick')"
        data-testid="link-scanner-pick"
        @update:model-value="onPickValue"
      />
    </div>
  </section>
</template>
