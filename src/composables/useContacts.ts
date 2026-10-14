import { invoke } from '@tauri-apps/api/core'
import type { Contact } from '@bindings/Contact'
import type { ContactGroup } from '@bindings/ContactGroup'
import type { ContactHeader } from '@bindings/ContactHeader'
import type { ContactInput } from '@bindings/ContactInput'
import type { ContactPatch } from '@bindings/ContactPatch'
import type { PickedFile } from '@bindings/PickedFile'

/**
 * Thin `invoke` wrappers of the address book commands (spec 051, `contracts/tauri-commands.md`).
 * Arguments travel in one `args` object; the backend decides who the caller is, never the window.
 */
export function useContacts() {
  const listAsync = () => invoke<ContactHeader[]>('contacts_list')

  const getAsync = (id: string) =>
    invoke<Contact>('contacts_get', { args: { id } })

  const createAsync = (input: ContactInput) =>
    invoke<Contact>('contacts_create', { args: { input } })

  const updateAsync = (id: string, patch: ContactPatch) =>
    invoke<Contact>('contacts_update', { args: { id, patch } })

  const deleteAsync = (ids: string[]) =>
    invoke('contacts_delete', { args: { ids } })

  /** Sets the photo from a chosen file; without a file it removes the photo. */
  const setPhotoAsync = (id: string, file: PickedFile | null) =>
    invoke<Contact>('contacts_set_photo', {
      args: file ? { id, file } : { id },
    })

  /** The stored JPEG of a photo. */
  const photoAsync = (hash: string) =>
    invoke<ArrayBuffer>('contacts_photo', { args: { hash } })

  const groupsAsync = () => invoke<ContactGroup[]>('contacts_groups')

  const groupSaveAsync = (name: string, id?: string) =>
    invoke<ContactGroup>('contacts_group_save', {
      args: id ? { id, name } : { name },
    })

  const groupDeleteAsync = (id: string) =>
    invoke('contacts_group_delete', { args: { id } })

  const groupMembersAsync = (
    groupId: string,
    add: string[],
    remove: string[] = [],
  ) =>
    invoke('contacts_group_members', {
      args: { groupId, add, remove },
    })

  return {
    listAsync,
    getAsync,
    createAsync,
    updateAsync,
    deleteAsync,
    setPhotoAsync,
    photoAsync,
    groupsAsync,
    groupSaveAsync,
    groupDeleteAsync,
    groupMembersAsync,
  }
}
