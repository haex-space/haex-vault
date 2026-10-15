import assert from 'node:assert/strict'
import { scenario } from '../lib/scenario.ts'
import { unwrap } from '../lib/flows.ts'
import { createContact } from '../lib/contacts.ts'
import {
  fixture,
  install,
  openFromLauncher,
  probeRequest,
} from '../lib/extensions.ts'
import { onDevice } from '../lib/platform/device-files.ts'

// Spec 051 US5-1 to US5-3 (quickstart `extension-contacts`): the probe declares reading the
// contacts of the group "Arbeit". The install preview shows it like any other permission; installed
// unticked, the first query asks, and "Erlauben" gives the probe Carla from "Arbeit" but never Anna,
// whom it cannot tell from a missing contact. A denied `*` takes everything away without asking.

interface Summary {
  id: string
  displayName: string
  emails: string[]
  groups: string[]
}

scenario('extension-contacts', { timeoutMs: 300_000 }, async (ctx) => {
  const group = await ctx.group({ users: { anna: ['laptop'] } })
  const page = group.device('anna/laptop').page

  const work = unwrap<{ id: string }>(
    'contacts_group_save',
    await page.invoke('contacts_group_save', { args: { name: 'Arbeit' } }),
  )
  const carla = await createContact(page, {
    head: { givenName: 'Carla', familyName: 'Dorn' },
    values: [{ kind: 'email', label: 'work', value: 'carla@example.org' }],
    groups: [work.id],
  })
  const anna = await createContact(page, {
    head: { givenName: 'Anna', familyName: 'Beck' },
    values: [{ kind: 'email', label: 'home', value: 'anna@example.org' }],
    groups: [],
  })

  const preview = unwrap<{
    declared: Array<{ kind: string; action: string; target: string }>
  }>(
    'extension_install_preview',
    await page.invoke('extension_install_preview', {
      file: onDevice(fixture('e2e', 'probe'), page),
    }),
  )
  assert.deepEqual(
    preview.declared
      .filter((row) => row.kind === 'contacts')
      .map((row) => [row.action, row.target]),
    [['read', 'Arbeit']],
  )
  ctx.step('the install preview shows reading the contacts of "Arbeit"')

  const probe = await install(page, fixture('e2e', 'probe'))
  await openFromLauncher(page, probe)
  const request = (method: string, params: unknown = {}) =>
    probeRequest(page, probe, `extension_contacts_${method}`, params)

  assert.equal((await request('list')).error?.code, 1004)
  await page.waitForDisplayed('extension-permission-request', 10_000)
  const asked = await page.exec<string>(
    `return document.querySelector('[data-testid="extension-permission-contacts-group"]')?.textContent ?? ''`,
  )
  assert.ok(asked.includes('Arbeit'), asked)
  await page.click('extension-permission-allow')
  ctx.step('the first query asks for the group "Arbeit"')

  const listed = (await request('list')).result as {
    contacts: Summary[]
    total: number
  }
  assert.equal(listed.total, 1)
  assert.deepEqual(
    listed.contacts.map((c) => [c.id, c.groups, c.emails]),
    [[carla, ['Arbeit'], ['carla@example.org']]],
  )
  const found = (await request('search', { query: 'example.org' })).result as {
    contacts: Summary[]
  }
  assert.deepEqual(
    found.contacts.map((c) => c.id),
    [carla],
  )
  const detail = (await request('read', { id: carla })).result as {
    displayName: string
    origin?: unknown
  }
  assert.equal(detail.displayName, 'Carla Dorn')
  assert.equal(detail.origin, undefined)
  ctx.step('allowed: only the contacts of "Arbeit"')

  const hidden = await request('read', { id: anna })
  const missing = await request('read', { id: 'no-such-contact' })
  assert.equal(hidden.error?.code, 1002)
  assert.deepEqual(hidden.error, missing.error, 'Anna looks like no contact')
  ctx.step('a contact outside the group cannot be told from a missing one')

  unwrap(
    'extension_permission_set',
    await page.invoke('extension_permission_set', {
      args: {
        extensionId: probe.id,
        kind: 'contacts',
        action: 'read',
        target: '*',
        status: 'denied',
      },
    }),
  )
  assert.equal((await request('list')).error?.code, 1002)
  assert.equal((await request('read', { id: carla })).error?.code, 1002)
  const prompting = await page.exec<boolean>(
    `return !!document.querySelector('[data-testid="extension-permission-request"]')`,
  )
  assert.equal(prompting, false, 'a denied permission does not ask')
  ctx.step('a denied * takes every contact away without asking')
})
