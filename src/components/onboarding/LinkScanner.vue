<script setup lang="ts">
/**
 * The camera of the link sheet on a desktop (spec 085, user story 2): a live preview inside the
 * sheet that reads the QR code of the main device. It opens when it is shown and gives the camera
 * back when it closes, the sheet closes, the window is left or an error ends it; nothing of the
 * picture is kept. What a frame holds is handed to `accept`, which says whether it was a link text:
 * only then the scan ends, anything else keeps looking. A camera that cannot be used is a toast
 * with the way out; pasting and typing the code stay possible.
 */
import { invoke } from '@tauri-apps/api/core'
import { toast } from 'vue-sonner'
import {
  CameraError,
  createCameraScan,
  type CameraDeps,
  type CameraErrorKind,
  type Frame,
} from '~/lib/sync/cameraScan'

interface Props {
  /** Reads a text from a frame; `true` when it was the code of a vault and the scan may end. */
  accept: (text: string) => Promise<boolean>
}

const props = defineProps<Props>()
const emit = defineEmits<{ close: [] }>()

const { t } = useI18n()
const region = ref<HTMLElement | null>(null)
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

/** The longest side of a looked-at frame: enough for a code at reading distance, cheap to decode. */
const FRAME_SIDE = 640

const deps: CameraDeps = {
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
  decode: (frame) =>
    decoder?.(frame.data, frame.width, frame.height)?.data ?? null,
  every: (ms, tick) => {
    const id = setInterval(tick, ms)
    return () => clearInterval(id)
  },
}
const scan = createCameraScan(deps)

/** Which system's words say where the camera is allowed. */
function system(): 'windows' | 'mac' | 'linux' {
  const platform = navigator.platform.toLowerCase()
  if (platform.includes('win')) return 'windows'
  if (platform.includes('mac')) return 'mac'
  return 'linux'
}

function failure(kind: CameraErrorKind): string {
  return kind === 'denied'
    ? t(`onboarding.link.camera.denied.${system()}`)
    : t(`onboarding.link.camera.${kind}`)
}

async function open(deviceId?: string) {
  const element = video.value
  if (!element) return
  try {
    decoder ??= (await import('jsqr')).default
    await scan.start(element, props.accept, deviceId)
    // Names of cameras are only there after the first grant.
    cameras.value = await deps.cameras()
  } catch (e: unknown) {
    const kind = e instanceof CameraError ? e.kind : 'failed'
    toast.error(failure(kind), { id: 'link-camera' })
    emit('close')
  }
}

async function onPick(deviceId: string) {
  cameraId.value = deviceId
  await scan.stop()
  await open(deviceId)
}

function onPickValue(value: unknown) {
  if (typeof value === 'string') void onPick(value)
}

/** The picture, the sheet or the window went away: the camera goes back. */
async function release() {
  await scan.stop()
  emit('close')
}

function onVisibility() {
  if (document.visibilityState === 'hidden') void release()
}

onMounted(() => {
  // Keyboard users land in the preview, on the button that closes it.
  region.value
    ?.querySelector<HTMLElement>('[data-testid="link-scanner-close"]')
    ?.focus()
  void open()
  hintTimer = setTimeout(() => (hint.value = true), 15_000)
  window.addEventListener('blur', release)
  document.addEventListener('visibilitychange', onVisibility)
})
onBeforeUnmount(() => {
  clearTimeout(hintTimer)
  window.removeEventListener('blur', release)
  document.removeEventListener('visibilitychange', onVisibility)
  void scan.stop()
})
</script>

<template>
  <section
    ref="region"
    class="space-y-2 rounded-md border p-3"
    data-testid="link-scanner"
    :aria-label="t('onboarding.link.camera.region')"
  >
    <video
      ref="video"
      class="aspect-video w-full rounded bg-black"
      muted
      playsinline
      :aria-label="t('onboarding.link.camera.preview')"
    />
    <p
      v-if="hint"
      class="text-sm text-muted-foreground"
      role="status"
      data-testid="link-scanner-hint"
    >
      {{ t('onboarding.link.camera.hint') }}
    </p>
    <div class="flex items-center gap-2">
      <UiSelect
        v-if="cameras.length > 1"
        :model-value="cameraId || cameras[0]?.deviceId || ''"
        :options="cameraOptions"
        :aria-label="t('onboarding.link.camera.pick')"
        data-testid="link-scanner-pick"
        @update:model-value="onPickValue"
      />
      <UiButton
        type="button"
        variant="outline"
        class="ml-auto"
        data-testid="link-scanner-close"
        @click="release"
      >
        {{ t('onboarding.link.camera.close') }}
      </UiButton>
    </div>
  </section>
</template>
