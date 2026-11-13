import { invoke } from '@tauri-apps/api/core'
import type { PassImportChoices } from '@bindings/PassImportChoices'
import type { PassImportPreview } from '@bindings/PassImportPreview'
import type { PassImportReport } from '@bindings/PassImportReport'
import type { PassImportSaved } from '@bindings/PassImportSaved'
import type { PickedFile } from '@bindings/PickedFile'
import type { Proposal } from '@bindings/Proposal'

/**
 * Thin `invoke` wrappers of the commands that import a pass from a file (spec 092,
 * `contracts/tauri-commands.md`). The file travels as the chosen path or address and is read by the
 * backend into memory; the window never sees its bytes and never a code value (FR-029, FR-035).
 */
export function usePassImport() {
  const previewAsync = (file: PickedFile, language: string) =>
    invoke<PassImportPreview>('passwords_pass_import_preview', {
      args: { file, language },
    })

  const saveAsync = (previewId: string, choices: PassImportChoices) =>
    invoke<PassImportSaved>('passwords_pass_import_save', {
      args: { previewId, choices },
    })

  /** Drops a preview the user did not save. */
  const cancelAsync = (previewId: string) =>
    invoke<null>('passwords_pass_import_cancel', { args: { previewId } })

  const placeProposalAsync = (
    passKind: string,
    hasDate: boolean,
    provider?: string,
  ) =>
    invoke<Proposal>('passwords_pass_place_proposal', {
      args: { passKind, hasDate, provider },
    })

  /** Writes the reports as text to the file the user chose. */
  const saveReportAsync = (reports: PassImportReport[], file: PickedFile) =>
    invoke<null>('passwords_pass_import_report_save', {
      args: { reports, file },
    })

  return {
    previewAsync,
    saveAsync,
    cancelAsync,
    placeProposalAsync,
    saveReportAsync,
  }
}
