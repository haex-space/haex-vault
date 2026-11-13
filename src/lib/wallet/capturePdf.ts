// Reading the codes of a PDF's pages (spec 092, FR-034, research R15): each page is drawn to a
// canvas with pdf.js, its PNG goes to the decoder in Rust, one page at a time so a long file does
// not fill the memory. Imported lazily by the capture dialog, so pdf.js stays out of the main
// bundle.
import { loadPdfData } from '~/lib/files/pdf'
import { pageScale, pageStep } from './capture.ts'

export type PdfSession = {
  pages: number
  /** The PNG of page `number` (1-based), drawn at the capture width within the pixel bound; `null`
   * for a page that declares no usable size. */
  pageImageAsync(number: number): Promise<Uint8Array | null>
  destroy(): void
}

/** Opens the PDF in memory; `data` is not used again by the caller. */
export async function openPdfAsync(data: Uint8Array): Promise<PdfSession> {
  const task = loadPdfData(data)
  const doc = await task.promise
  return {
    pages: doc.numPages,
    async pageImageAsync(number: number) {
      const page = await doc.getPage(number)
      const base = page.getViewport({ scale: 1 })
      const scale = pageScale(base.width, base.height)
      if (scale === null) {
        page.cleanup()
        return null
      }
      const viewport = page.getViewport({ scale })
      const canvas = document.createElement('canvas')
      canvas.width = Math.floor(viewport.width)
      canvas.height = Math.floor(viewport.height)
      try {
        if (canvas.width < 1 || canvas.height < 1) return null
        await page.render({ canvas, viewport }).promise
        const blob = await new Promise<Blob | null>((resolve) =>
          canvas.toBlob(resolve, 'image/png'),
        )
        if (blob === null) throw new Error('the page could not be drawn')
        return new Uint8Array(await blob.arrayBuffer())
      } finally {
        page.cleanup()
        // Free the pixels at once: a long file draws many pages.
        canvas.width = 0
        canvas.height = 0
      }
    },
    destroy() {
      void task.destroy().catch(() => {})
    },
  }
}

export { pageStep }
