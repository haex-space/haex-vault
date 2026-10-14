// Helpers for the address book scenarios (spec 051): opening the window and reading what the
// backend holds through its commands, so a scenario checks data, not pixels. Contacts are made
// through `contacts_create` where the editor is not what is being tested.
import { unwrap } from './flows.ts'
import { resizeAppWindow } from './settings.ts'
import type { FlowInstance } from './flows.ts'
import type { WaitContext } from './sync-flows.ts'

/** A contact header as `contacts_list` returns it, reduced to what the scenarios read. */
export interface ContactHeader {
  id: string
  name: string
  phones: string[]
  emails: string[]
  groups: string[]
}

/** Opens the address book from the launcher, fits its window into the virtual screen and waits
 * for its toolbar. */
export async function openContacts(instance: FlowInstance): Promise<void> {
  await instance.click('open-launcher')
  await instance.click('[data-app-id="system.contacts"]')
  await instance.waitForDisplayed('contacts-search')
  await resizeAppWindow(instance, 'system.contacts', 760, 560)
  await instance.waitForDisplayed('contacts-new')
}

/** What the backend holds right now. */
export async function contactHeaders(
  instance: Pick<FlowInstance, 'invoke'>,
): Promise<ContactHeader[]> {
  return unwrap<ContactHeader[]>(
    'contacts_list',
    await instance.invoke('contacts_list'),
  )
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
