/**
 * The full text of a long message (spec 072 FR-012, FR-012a): a text that does not fit one wrap
 * travels as an `inline` file and the message's own text is only a preview. What the window offers
 * at the end of the preview follows from where this device's copy of that file stands.
 */

/** The parts of a message's attachment that decide it; `ChatPeerAttachment` has them. */
export interface TextFile {
  id: string
  inline: boolean
  /** On this device: `pending`, `downloading`, `stored`, `failed`, `gone`; `null` before it looked. */
  state: string | null
  bytesDone: number
  size: number
  error: string | null
}

/** What stands at the end of a long text. */
export type TextControl =
  /** The full text is on this device: "Weiterlesen" reads it, "Weniger anzeigen" folds it. */
  | { kind: 'toggle' }
  /** The copy is coming (or this device did not look yet); `percent` when known. */
  | { kind: 'loading'; percent: number | null }
  /** Above this device's automatic limit: the user asks for it. */
  | { kind: 'download' }
  /** The last download failed. */
  | { kind: 'retry'; error: string | null }
  /** The file is no longer on the server (FR-035). */
  | { kind: 'gone' }

/** The file that holds the full text of a message, if the message has one. */
export function textFile<T extends { inline: boolean }>(
  attachments: readonly T[],
): T | null {
  return attachments.find((attachment) => attachment.inline) ?? null
}

/** What to offer for `file`, by the state of its copy on this device. */
export function textControl(file: TextFile): TextControl {
  switch (file.state) {
    case 'stored':
      return { kind: 'toggle' }
    case 'pending':
      return { kind: 'download' }
    case 'failed':
      return { kind: 'retry', error: file.error }
    case 'gone':
      return { kind: 'gone' }
    case 'downloading':
      return {
        kind: 'loading',
        percent:
          file.size > 0
            ? Math.min(100, Math.floor((file.bytesDone * 100) / file.size))
            : null,
      }
    default:
      return { kind: 'loading', percent: null }
  }
}
