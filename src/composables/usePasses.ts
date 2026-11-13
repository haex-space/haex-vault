import { invoke } from '@tauri-apps/api/core'
import type { CodeCheck } from '@bindings/CodeCheck'
import type { CodeSvg } from '@bindings/CodeSvg'
import type { CodeView } from '@bindings/CodeView'
import type { ItemDetail } from '@bindings/ItemDetail'
import type { PassInput } from '@bindings/PassInput'
import type { PassPatch } from '@bindings/PassPatch'

/**
 * Thin `invoke` wrappers of the pass commands (spec 092, `contracts/tauri-commands.md`). Arguments
 * travel in one `args` object; the backend decides who the caller is. A code value comes back only
 * from `codesAsync` (one pass, the user's own screen); a drawing comes from `codeSvgAsync`.
 */
export function usePasses() {
  const createPassAsync = (input: PassInput, groupId?: string) =>
    invoke<ItemDetail>('passwords_pass_create', { args: { input, groupId } })

  const updatePassAsync = (id: string, updatedAt: string, patch: PassPatch) =>
    invoke<ItemDetail>('passwords_pass_update', {
      args: { id, updatedAt, patch },
    })

  /** Sets or clears the entry a pass belongs to; a loop is refused with `parent_loop`. */
  const setParentAsync = (
    id: string,
    updatedAt: string,
    parentId: string | null,
  ) =>
    invoke<ItemDetail>('passwords_pass_set_parent', {
      args: { id, updatedAt, parentId },
    })

  const codesAsync = (id: string) =>
    invoke<CodeView[]>('passwords_pass_codes', { args: { id } })

  const codeSvgAsync = (id: string, codeId: string) =>
    invoke<CodeSvg>('passwords_pass_code_svg', { args: { id, codeId } })

  /** The check the editor runs while typing; the one rule lives in Rust (research R3). */
  const checkCodeAsync = (format: string, value: string) =>
    invoke<CodeCheck>('passwords_pass_code_check', {
      args: { format, value },
    })

  return {
    createPassAsync,
    updatePassAsync,
    setParentAsync,
    codesAsync,
    codeSvgAsync,
    checkCodeAsync,
  }
}
