// Which dialog a file shared to holzi opens (spec 092, FR-037). Pure, so
// `scripts/check-wallet-shared.ts` runs it without the generated bindings.

/** What a shared file is (`SharedKind` of the bindings). */
export type SharedKind = 'pkpass' | 'pdf' | 'image'

/** A waiting file, the parts routing needs (`SharedFileView` of the bindings). */
export type SharedItem = { path: string; kind: SharedKind }

/** A shared PDF or picture waiting for the capture dialog; `file` is a path (`PickedFile`). */
export type SharedCapture = { file: string; kind: 'pdf' | 'image' }

/** Where waiting files go: pass files to the import dialog, pictures and PDFs to the capture. */
export function routeShared(files: readonly SharedItem[]): {
  imports: string[]
  captures: SharedCapture[]
} {
  const imports: string[] = []
  const captures: SharedCapture[] = []
  for (const shared of files) {
    if (shared.kind === 'pkpass') imports.push(shared.path)
    else captures.push({ file: shared.path, kind: shared.kind })
  }
  return { imports, captures }
}
