<script setup lang="ts">
/**
 * THROWAWAY (spec 085 task T006): opens the camera in this webview, decodes QR codes with jsqr and
 * reports what happened, so each OS's webview can be judged before the real scanner is built.
 * Never merged; lives on branch 085-a-spike only.
 */
import { invoke } from '@tauri-apps/api/core'
import jsQR from 'jsqr'

const video = ref<HTMLVideoElement | null>(null)
const running = ref(false)
const error = ref('')
const devices = ref<string[]>([])
const lastText = ref('')
const decodes = ref(0)
const frames = ref(0)
const fps = ref(0)
const firstDecodeMs = ref<number | null>(null)
const startedAt = ref(0)
const log = ref<string[]>([])

const facts = {
  userAgent: navigator.userAgent,
  platform: navigator.platform,
  location: location.href,
  secureContext: window.isSecureContext,
  mediaDevices: typeof navigator.mediaDevices !== 'undefined',
  getUserMedia: typeof navigator.mediaDevices?.getUserMedia === 'function',
  barcodeDetector: 'BarcodeDetector' in window,
}

let stream: MediaStream | null = null
let timer: ReturnType<typeof setInterval> | undefined
let windowStart = 0
let windowFrames = 0
const canvas = document.createElement('canvas')
const context = canvas.getContext('2d', { willReadFrequently: true })

function note(line: string) {
  log.value = [
    ...log.value,
    `${((Date.now() - startedAt.value) / 1000).toFixed(1)} s  ${line}`,
  ].slice(-30)
}

async function start() {
  error.value = ''
  lastText.value = ''
  decodes.value = 0
  frames.value = 0
  firstDecodeMs.value = null
  log.value = []
  startedAt.value = Date.now()
  try {
    await invoke('camera_scan_begin')
    note('gate open')
    if (!navigator.mediaDevices?.getUserMedia)
      throw new Error('navigator.mediaDevices.getUserMedia is missing')
    stream = await navigator.mediaDevices.getUserMedia({
      video: { facingMode: 'environment' },
    })
    note('getUserMedia resolved')
    const all = await navigator.mediaDevices.enumerateDevices()
    devices.value = all
      .filter((d) => d.kind === 'videoinput')
      .map((d) => d.label || '(no label)')
    if (video.value) {
      video.value.srcObject = stream
      await video.value.play()
      note(`video playing ${video.value.videoWidth}x${video.value.videoHeight}`)
    }
    running.value = true
    windowStart = Date.now()
    windowFrames = 0
    timer = setInterval(tick, 100)
  } catch (cause) {
    const e = cause as { name?: string; message?: string }
    error.value = `${e.name ?? 'Error'}: ${e.message ?? String(cause)}`
    note(`failed: ${error.value}`)
    await stop()
  }
}

function tick() {
  const el = video.value
  if (!el || !context || el.videoWidth === 0) return
  const scale = Math.min(1, 640 / Math.max(el.videoWidth, el.videoHeight))
  canvas.width = Math.round(el.videoWidth * scale)
  canvas.height = Math.round(el.videoHeight * scale)
  context.drawImage(el, 0, 0, canvas.width, canvas.height)
  const image = context.getImageData(0, 0, canvas.width, canvas.height)
  const found = jsQR(image.data, image.width, image.height)
  frames.value += 1
  windowFrames += 1
  const now = Date.now()
  if (now - windowStart >= 1000) {
    fps.value = Math.round((windowFrames * 1000) / (now - windowStart))
    windowStart = now
    windowFrames = 0
  }
  if (found && found.data) {
    decodes.value += 1
    lastText.value = found.data
    if (firstDecodeMs.value === null) {
      firstDecodeMs.value = now - startedAt.value
      note(`first decode after ${firstDecodeMs.value} ms`)
    }
  }
}

async function stop() {
  clearInterval(timer)
  timer = undefined
  const tracks = stream?.getTracks() ?? []
  for (const track of tracks) track.stop()
  stream = null
  if (video.value) video.value.srcObject = null
  running.value = false
  try {
    await invoke('camera_scan_end')
    note(`stopped, ${tracks.length} track(s) released, gate closed`)
  } catch (cause) {
    note(`gate end failed: ${String(cause)}`)
  }
}

const report = computed(() =>
  JSON.stringify(
    {
      facts,
      error: error.value || null,
      devices: devices.value,
      frames: frames.value,
      fps: fps.value,
      decodes: decodes.value,
      firstDecodeMs: firstDecodeMs.value,
      lastText: lastText.value.slice(0, 60),
      log: log.value,
    },
    null,
    2,
  ),
)

async function copyReport() {
  await navigator.clipboard.writeText(report.value)
}

onBeforeUnmount(() => {
  void stop()
})
</script>

<template>
  <section
    class="fixed top-2 right-2 z-50 flex w-96 flex-col gap-2 rounded-lg border bg-background p-3 text-xs shadow-lg"
    data-testid="link-camera-spike"
  >
    <p class="font-semibold">Spike 085: camera in this webview</p>
    <video
      ref="video"
      class="aspect-video w-full rounded bg-black"
      muted
      playsinline
    />
    <div class="flex gap-2">
      <button
        class="rounded border px-2 py-1"
        :disabled="running"
        @click="start"
      >
        Start camera
      </button>
      <button
        class="rounded border px-2 py-1"
        :disabled="!running"
        @click="stop"
      >
        Stop
      </button>
      <button class="rounded border px-2 py-1" @click="copyReport">
        Copy report
      </button>
    </div>
    <pre class="max-h-72 overflow-auto whitespace-pre-wrap">{{ report }}</pre>
  </section>
</template>
