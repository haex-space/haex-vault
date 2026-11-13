/**
 * Loading and drawing PDFs with pdf.js (spec 044 FR-010, T038), shared by the PDF viewer and the
 * platform probe (T041), so the probe checks what the viewer runs. pdf.js reads the file from the
 * media server in ranges of 1 MiB and fetches only what a page needs (pdf.js keeps reading the
 * whole file unless streaming is off as well). No wasm (the CSP has no `wasm-unsafe-eval`; pdf.js
 * tests for eval itself): JBIG2 and JPEG 2000 images decode with the JavaScript fallbacks from
 * `/pdfjs/wasm/`. CMaps and standard fonts come from `/pdfjs/` too (copied there by
 * `nuxt.config.ts`) and are fetched by the page, not the worker. Imported only
 * lazily, so pdf.js stays out of the main bundle. The legacy build: Android System WebView 124
 * (the CI emulator, and phones that update their web view late) has no `URL.parse`, which pdf.js 6
 * calls; the legacy build brings it along (platform probe, PR #341).
 */
import {
  getDocument,
  GlobalWorkerOptions,
  type PDFDocumentLoadingTask,
  type PDFDocumentProxy,
  type RenderTask,
} from 'pdfjs-dist/legacy/build/pdf.mjs'
import workerUrl from 'pdfjs-dist/legacy/build/pdf.worker.min.mjs?url'

GlobalWorkerOptions.workerSrc = workerUrl

const asset = (path: string) => new URL(path, window.location.href).href

/** The largest canvas drawn, as pdf.js's own viewer bounds it; beyond, CSS stretches the page. */
const MAX_CANVAS_PIXELS = 2 ** 24

/** Starts loading the PDF at `url`; `destroy()` on the task ends it. */
export function loadPdf(url: string): PDFDocumentLoadingTask {
  return getDocument({
    url,
    rangeChunkSize: 1024 * 1024,
    disableAutoFetch: true,
    disableStream: true,
    useWasm: false,
    wasmUrl: asset('/pdfjs/wasm/'),
    useWorkerFetch: false,
    cMapUrl: asset('/pdfjs/cmaps/'),
    standardFontDataUrl: asset('/pdfjs/standard_fonts/'),
  })
}

/**
 * Starts loading a PDF held in memory (the capture of a code from a PDF the person chose, spec 092
 * FR-034): the same settings as [`loadPdf`], with the bytes in place of an address. pdf.js takes
 * ownership of the array, so the caller passes a copy it does not need again.
 */
export function loadPdfData(data: Uint8Array): PDFDocumentLoadingTask {
  return getDocument({
    data,
    useWasm: false,
    wasmUrl: asset('/pdfjs/wasm/'),
    useWorkerFetch: false,
    cMapUrl: asset('/pdfjs/cmaps/'),
    standardFontDataUrl: asset('/pdfjs/standard_fonts/'),
  })
}

/** Draws page `number` of `doc` into `canvas` at `scale` (sharp on high-density screens). */
export async function renderPdfPage(
  doc: PDFDocumentProxy,
  number: number,
  canvas: HTMLCanvasElement,
  scale: number,
): Promise<RenderTask> {
  const page = await doc.getPage(number)
  const css = page.getViewport({ scale })
  const ratio = Math.min(
    window.devicePixelRatio || 1,
    Math.sqrt(MAX_CANVAS_PIXELS / (css.width * css.height)),
  )
  const viewport = page.getViewport({ scale: scale * ratio })
  canvas.width = Math.floor(viewport.width)
  canvas.height = Math.floor(viewport.height)
  canvas.style.width = `${css.width}px`
  canvas.style.height = `${css.height}px`
  return page.render({ canvas, viewport })
}
