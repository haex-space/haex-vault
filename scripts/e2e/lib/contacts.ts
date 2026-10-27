// Helpers for the address book scenarios (spec 051): opening the window and reading what the
// backend holds through its commands, so a scenario checks data, not pixels. Contacts are made
// through `contacts_create` where the editor is not what is being tested.
import assert from 'node:assert/strict'
import { unwrap } from './flows.ts'
import { resizeAppWindow, runAction, waitForLocation } from './settings.ts'
import type { FlowInstance } from './flows.ts'
import type { ContactControls } from './platform/android-contacts.ts'
import type { WaitContext } from './sync-flows.ts'

/** A contact header as `contacts_list` returns it, reduced to what the scenarios read. */
export interface ContactHeader {
  id: string
  name: string
  phones: string[]
  emails: string[]
  groups: string[]
  /** The phones the contact goes to. */
  targets: string[]
  /** The fixed contact of the vault user, which the spec 051 scenarios do not exercise. */
  isSelf: boolean
}

/** Opens the address book from the launcher, fits its window into the virtual screen and waits
 * for its toolbar. */
export async function openContacts(instance: FlowInstance): Promise<void> {
  await instance.click('open-launcher')
  await instance.click('[data-app-id="system.contacts"]')
  await instance.waitForDisplayed('contacts-search')
  await resizeAppWindow(instance, 'system.contacts', 760, 560)
  await instance.waitForDisplayed('contacts-add')
}

/** What the backend holds right now. */
export async function contactHeaders(
  instance: Pick<FlowInstance, 'invoke'>,
  includeSelf = false,
): Promise<ContactHeader[]> {
  const headers = unwrap<ContactHeader[]>(
    'contacts_list',
    await instance.invoke('contacts_list'),
  )
  return includeSelf ? headers : headers.filter((header) => !header.isSelf)
}

/** Creates a contact through the command and returns its id. */
export async function createContact(
  instance: Pick<FlowInstance, 'invoke'>,
  input: Record<string, unknown>,
): Promise<string> {
  return unwrap<{ id: string }>(
    'contacts_create',
    await instance.invoke('contacts_create', { args: { input } }),
  ).id
}

/** Waits until the device holds exactly the contacts with these names, in any order. */
export async function expectContacts(
  ctx: WaitContext,
  instance: Pick<FlowInstance, 'invoke'>,
  names: string[],
  what: string,
): Promise<void> {
  const wanted = [...names].sort()
  await ctx.waitFor(
    what,
    async () => {
      const held = (await contactHeaders(instance))
        .map((header) => header.name)
        .sort()
      return JSON.stringify(held) === JSON.stringify(wanted)
    },
    { timeoutMs: 40_000, fixed: true },
  )
}

/** Opens Einstellungen › Kontakte. */
export async function openContactsSettings(
  instance: FlowInstance,
): Promise<void> {
  const opened = await runAction(instance, 'wm.app.open', {
    appId: 'system.settings',
    at: '/contacts',
  })
  assert.ok(opened.ok, JSON.stringify(opened))
  await waitForLocation(instance, 'contacts')
}

/** Calls a command of the sync with the phone and answers its result. */
export async function phoneCommand<T>(
  instance: Pick<FlowInstance, 'invoke'>,
  command: string,
  args?: Record<string, unknown>,
): Promise<T> {
  return unwrap<T>(
    command,
    await instance.invoke(command, args ? { args } : undefined),
  )
}

/** Switches the sync on through its commands and confirms the overview without merging. */
export async function startPhoneSync(
  instance: Pick<FlowInstance, 'invoke'>,
): Promise<void> {
  await phoneCommand(instance, 'contacts_phone_enable')
  await phoneCommand(instance, 'contacts_phone_confirm', { decisions: [] })
}

/** The address book controls of a group's device that runs on the phone of an Android run. */
export function phoneOf(page: object): ContactControls {
  const controls = (page as { phone?: ContactControls }).phone
  assert.ok(controls !== undefined, 'the device runs on the phone')
  return controls
}

/** The phone's own device id as the vault knows it: the one phone with the sync on. */
export async function phoneDevice(
  instance: Pick<FlowInstance, 'invoke'>,
): Promise<string> {
  const phones = await phoneCommand<Array<{ device: string }>>(
    instance,
    'contacts_phones',
  )
  assert.equal(phones.length, 1, JSON.stringify(phones))
  return phones[0]!.device
}
