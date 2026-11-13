import { invoke } from '@tauri-apps/api/core'
import type { PickedFile } from '@bindings/PickedFile'
import type { ScannedCode } from '@bindings/ScannedCode'

/**
 * Thin `invoke` wrappers of the commands that read a code for a pass (spec 092, FR-034, research
 * R15). The decoding is in Rust; the window sends a picture (a photo, a rendered page, a frame) or a
 * chosen file and gets back what was found, to show. Nothing is saved by these calls.
 */
export function usePassCapture() {
  /** The codes in a picture the window holds (the bytes travel as the raw body). */
  const scanImageAsync = (image: Uint8Array) =>
    invoke<ScannedCode[]>('passwords_pass_codes_scan', image)

  /** The codes in a photo the person chose. */
  const scanFileAsync = (file: PickedFile) =>
    invoke<ScannedCode[]>('passwords_pass_codes_scan_file', { args: { file } })

  /** The bytes of a PDF the person chose, to render in the window; refused when it is no PDF. */
  const pdfBytesAsync = async (file: PickedFile): Promise<Uint8Array> =>
    new Uint8Array(
      await invoke<ArrayBuffer>('passwords_pass_capture_pdf', {
        args: { file },
      }),
    )

  return { scanImageAsync, scanFileAsync, pdfBytesAsync }
}
