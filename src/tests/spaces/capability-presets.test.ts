/**
 * Tests for the role presets in `capsFromSingle` — the shape of every
 * delegated UCAN the frontend mints.
 *
 * These pin the normative table:
 *
 *   | role    | read              | write     | invite   | admin      |
 *   |---------|-------------------|-----------|----------|------------|
 *   | reader  | delegatable false | —         | —        | —          |
 *   | writer  | false             | false     | —        | —          |
 *   | inviter | true              | —         | true     | —          |
 *   | admin   | true              | true      | true     | false      |
 *
 * Two regressions are locked down here:
 *
 * - inviter used to be `read(false) invite(true)`, which made the invite
 *   capability inert: enforceDelegatable reports the first offender in
 *   SPACE_CAP_ORDER, so the holder tripped on `read` and could grant nothing.
 * - admin used to be `read(false) admin(true)` — wrong on both bits. The
 *   delegatable admin permitted admin proliferation, and the missing
 *   write/invite stripped what an admin needs in order to hand anything out.
 */

import { describe, it, expect } from 'vitest'
import { enforceDelegatable, spaceCapabilitySet } from '@haex-space/ucan'
import { capsFromSingle } from '@/utils/auth/ucanStore'

describe('capsFromSingle role presets', () => {
  it('reader holds only a non-delegatable read', () => {
    expect(capsFromSingle('read')).toEqual([
      { cap: 'read', delegatable: false },
    ])
  })

  it('writer holds read and write, neither delegatable', () => {
    expect(capsFromSingle('write')).toEqual([
      { cap: 'read', delegatable: false },
      { cap: 'write', delegatable: false },
    ])
  })

  it('inviter may delegate read and invite', () => {
    expect(capsFromSingle('invite')).toEqual([
      { cap: 'read', delegatable: true },
      { cap: 'invite', delegatable: true },
    ])
  })

  it('admin may delegate read/write/invite but not admin', () => {
    expect(capsFromSingle('admin')).toEqual([
      { cap: 'read', delegatable: true },
      { cap: 'write', delegatable: true },
      { cap: 'invite', delegatable: true },
      { cap: 'admin', delegatable: false },
    ])
  })
})

describe('capsFromSingle presets are actually usable', () => {
  // The regression test for the inert-invite bug: an inviter must be able to
  // hand out a reader preset, and must not be able to hand out a writer one.
  it('an inviter can delegate a reader preset', () => {
    expect(enforceDelegatable(capsFromSingle('invite'), capsFromSingle('read')))
      .toBeNull()
  })

  it('an inviter cannot delegate a writer preset', () => {
    expect(enforceDelegatable(capsFromSingle('invite'), capsFromSingle('write')))
      .toEqual({ kind: 'missing', cap: 'write' })
  })

  it('an admin can delegate reader, writer and inviter presets', () => {
    const admin = capsFromSingle('admin')
    expect(enforceDelegatable(admin, capsFromSingle('read'))).toBeNull()
    expect(enforceDelegatable(admin, capsFromSingle('write'))).toBeNull()
    expect(enforceDelegatable(admin, capsFromSingle('invite'))).toBeNull()
  })

  it('an admin cannot mint another admin', () => {
    expect(enforceDelegatable(capsFromSingle('admin'), capsFromSingle('admin')))
      .toEqual({ kind: 'not_delegatable', cap: 'admin' })
  })

  it('the space owner can delegate every preset, admin included', () => {
    // The owner root set, as minted by createRootUcanAsync.
    const owner = spaceCapabilitySet()
      .read(true).write(true).invite(true).admin(true).build()

    expect(enforceDelegatable(owner, capsFromSingle('read'))).toBeNull()
    expect(enforceDelegatable(owner, capsFromSingle('write'))).toBeNull()
    expect(enforceDelegatable(owner, capsFromSingle('invite'))).toBeNull()
    expect(enforceDelegatable(owner, capsFromSingle('admin'))).toBeNull()
  })

  it('a reader and a writer can delegate nothing', () => {
    // Neither carries invite, so neither should ever reach a grant boundary.
    for (const holder of ['read', 'write'] as const) {
      for (const target of ['read', 'write', 'invite', 'admin'] as const) {
        expect(enforceDelegatable(capsFromSingle(holder), capsFromSingle(target)))
          .not.toBeNull()
      }
    }
  })
})
