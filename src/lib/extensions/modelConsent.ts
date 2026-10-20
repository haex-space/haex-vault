// The consent to send a job outside this device (spec 055 US4): what a step would send, as the
// question lists it. Pure: `components/extensions/ModelDestination.vue` uses it,
// `scripts/check-extensions-model-consent.ts` tests it. Kinds and sizes only, never the content.

import { humanBytes } from '../models/format.ts'

export const MODEL_DESTINATION_KIND = 'modelDestination'

/** What a step would send, as Rust sends it. */
export interface Transfer {
  kinds: string[]
  textBytes: number
  imageCount: number
  imageBytes: number
  earlierResult: boolean
}

/** One line of the list: its i18n key, the named values, and the count for a plural. */
export interface TransferLine {
  key: string
  params: Record<string, string | number>
  plural?: number
}

/** A size for the question; a short text is "< 1 KB", not "0 KB". */
export function sizeLabel(bytes: number): string {
  return bytes < 1024 ? '< 1 KB' : humanBytes(bytes)
}

/** The text, the images and the result of an earlier step, each when the step sends it. */
export function transferLines(transfer: Transfer): TransferLine[] {
  const base = 'extensions.permissionRequest.modelDestination'
  const lines: TransferLine[] = [
    { key: `${base}.text`, params: { size: sizeLabel(transfer.textBytes) } },
  ]
  if (transfer.imageCount > 0)
    lines.push({
      key: `${base}.images`,
      params: {
        count: transfer.imageCount,
        size: sizeLabel(transfer.imageBytes),
      },
      plural: transfer.imageCount,
    })
  if (transfer.earlierResult)
    lines.push({ key: `${base}.earlierResult`, params: {} })
  return lines
}
