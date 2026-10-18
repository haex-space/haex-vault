<script setup lang="ts">
/**
 * PDFs (spec 044 FR-009, FR-010; after haex-vault 2.x (Legacy) `src/components/haex/system/files/PdfViewer.vue`):
 * one page at a time with page buttons and zoom, the same on every platform. Loading and drawing
 * live in `lib/files/pdf.ts`. Used as `LazyFilesPdfViewer`, so pdf.js stays out of the main bundle.
 * Takes the focus when it opens, so PageUp and PageDown turn pages; the other keys reach the viewer.
 */
import type {
  PDFDocumentLoadingTask,
  PDFDocumentProxy,
  RenderTask,
} from 'pdfjs-dist/legacy/build/pdf.mjs'

import { loadPdf, renderPdfPage } from '~/lib/files/pdf'

const ZOOMS = [0.5, 0.75, 1, 1.25, 1.5, 2, 3]
const FIT = ZOOMS.indexOf(1)

const props = defineProps<{ url: string }>()
const emit = defineEmits<{ failed: [] }>()

const { t } = useI18n()

const pages = ref(0)
const page = ref(1)
const zoom = ref(FIT)
const canvas = useTemplateRef<HTMLCanvasElement>('canvas')
const root = useTemplateRef<HTMLElement>('root')

let task: PDFDocumentLoadingTask | null = null
let doc: PDFDocumentProxy | null = null
let rendering: RenderTask | null = null
/** The page on the canvas; leaving it frees what pdf.js keeps for it. */
let drawnPage = 0

async function load(url: string) {
  rendering?.cancel()
  void task?.destroy().catch(() => {})
  doc = null
  drawnPage = 0
  pages.value = 0
  page.value = 1
  const current = loadPdf(url)
  task = current
  try {
    const loaded = await current.promise
    if (task !== current) return
    doc = loaded
    pages.value = loaded.numPages
  } catch {
    if (task === current) emit('failed')
  }
}

/**
 * Draws one page at a time: pdf.js refuses a second render on a canvas still in use. Only the
 * latest request draws; the ones it overtook while waiting are skipped.
 */
let drawn: Promise<void> = Promise.resolve()
let requested = 0

function render() {
  rendering?.cancel()
  const request = ++requested
  drawn = drawn.then(() => (request === requested ? draw() : undefined))
}

async function draw() {
  const target = canvas.value
  const scale = ZOOMS[zoom.value]
  const number = page.value
  const source = doc
  if (!source || !target || scale === undefined) return
  try {
    const current = await renderPdfPage(source, number, target, scale)
    rendering = current
    await current.promise
    target.dataset.renderedPage = String(number)
    if (drawnPage !== 0 && drawnPage !== number)
      void source
        .getPage(drawnPage)
        .then((left) => left.cleanup())
        .catch(() => {})
    drawnPage = number
  } catch (error) {
    // Cancelled by a newer page or zoom, or the document went away.
    if (doc !== source) return
    if (error instanceof Error && error.name === 'RenderingCancelledException')
      return
    console.error('[files] drawing the PDF page failed', error)
    emit('failed')
  }
}

watch(() => props.url, load, { immediate: true })
watch([pages, page, zoom], render, { flush: 'post' })
onMounted(() => root.value?.focus())
onBeforeUnmount(() => {
  doc = null
  rendering?.cancel()
  void task?.destroy().catch(() => {})
})

function turn(by: number) {
  page.value = Math.min(Math.max(page.value + by, 1), pages.value)
}

function onKeydown(event: KeyboardEvent) {
  if (event.key === 'PageDown') turn(1)
  else if (event.key === 'PageUp') turn(-1)
}
</script>

<template>
  <div
    ref="root"
    class="flex min-h-full flex-col items-center outline-none"
    tabindex="-1"
    data-testid="files-viewer-pdf"
    @keydown="onKeydown"
  >
    <div
      class="sticky top-0 z-10 flex items-center gap-1 rounded-b-md border border-t-0 bg-background px-2 py-1"
    >
      <UiButton
        variant="ghost"
        size="icon"
        :disabled="page <= 1"
        :aria-label="t('files.viewer.pdf.previous')"
        :tooltip="t('files.viewer.pdf.previous')"
        data-testid="files-pdf-previous"
        @click="turn(-1)"
      >
        <Icon name="lucide:chevron-up" class="size-4" />
      </UiButton>
      <span
        class="min-w-20 text-center text-sm tabular-nums"
        data-testid="files-pdf-page"
        >{{ t('files.viewer.pdf.page', { page, pages }) }}</span
      >
      <UiButton
        variant="ghost"
        size="icon"
        :disabled="page >= pages"
        :aria-label="t('files.viewer.pdf.next')"
        :tooltip="t('files.viewer.pdf.next')"
        data-testid="files-pdf-next"
        @click="turn(1)"
      >
        <Icon name="lucide:chevron-down" class="size-4" />
      </UiButton>
      <UiButton
        variant="ghost"
        size="icon"
        :disabled="zoom <= 0"
        :aria-label="t('files.viewer.pdf.zoomOut')"
        :tooltip="t('files.viewer.pdf.zoomOut')"
        data-testid="files-pdf-zoom-out"
        @click="zoom -= 1"
      >
        <Icon name="lucide:zoom-out" class="size-4" />
      </UiButton>
      <span class="min-w-12 text-center text-sm tabular-nums"
        >{{ Math.round((ZOOMS[zoom] ?? 1) * 100) }} %</span
      >
      <UiButton
        variant="ghost"
        size="icon"
        :disabled="zoom >= ZOOMS.length - 1"
        :aria-label="t('files.viewer.pdf.zoomIn')"
        :tooltip="t('files.viewer.pdf.zoomIn')"
        data-testid="files-pdf-zoom-in"
        @click="zoom += 1"
      >
        <Icon name="lucide:zoom-in" class="size-4" />
      </UiButton>
    </div>
    <canvas
      ref="canvas"
      class="my-4 shadow-md"
      data-testid="files-pdf-canvas"
    />
  </div>
</template>
