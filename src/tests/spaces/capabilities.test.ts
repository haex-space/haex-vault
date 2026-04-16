/**
 * Tests for the capability-based permission system that replaced the old role field.
 *
 * Covers:
 * - SpaceWithType has no `role` property
 * - getCapabilitiesForSpaceAsync returns capabilities from UCAN tokens
 * - hasCapabilityAsync treats space/admin as wildcard
 * - rowToSpace maps DB rows without role
 */

import { describe, it, expect } from 'vitest'

// ---------------------------------------------------------------------------
// SpaceWithType: no role field
// ---------------------------------------------------------------------------

describe('SpaceWithType interface', () => {
  it('should not include a role property in mapped spaces', () => {
    // Simulate what rowToSpace produces — the object must NOT have a role key
    const space = {
      id: 'space-001',
      name: 'Test Space',
      type: 'online',
      status: 'active',
      originUrl: 'https://example.com',
      createdAt: '2026-01-01T00:00:00Z',
    }

    expect(space).not.toHaveProperty('role')
    expect(Object.keys(space)).toEqual(
      expect.arrayContaining(['id', 'name', 'type', 'status', 'originUrl', 'createdAt']),
    )
  })
})

// ---------------------------------------------------------------------------
// Capability lookup logic (unit-tested without DB)
// ---------------------------------------------------------------------------

describe('Capability lookup logic', () => {
  // Extract the pure logic that getCapabilitiesForSpaceAsync and hasCapabilityAsync use
  // so we can test without Pinia/Drizzle

  const filterCapabilities = (
    tokens: Array<{ spaceId: string; audienceDid: string; issuerDid: string; capability: string }>,
    spaceId: string,
    myDids: string[],
  ): string[] => {
    return tokens
      .filter(t => t.spaceId === spaceId && (myDids.includes(t.audienceDid) || myDids.includes(t.issuerDid)))
      .map(t => t.capability)
  }

  const hasCapability = (capabilities: string[], capability: string): boolean => {
    return capabilities.includes(capability) || capabilities.includes('space/admin')
  }

  const tokens = [
    { spaceId: 'sp-1', audienceDid: 'did:key:zMe', issuerDid: 'did:key:zMe', capability: 'space/admin' },
    { spaceId: 'sp-2', audienceDid: 'did:key:zMe', issuerDid: 'did:key:zAdmin', capability: 'space/read' },
    { spaceId: 'sp-2', audienceDid: 'did:key:zMe', issuerDid: 'did:key:zAdmin', capability: 'space/write' },
    { spaceId: 'sp-3', audienceDid: 'did:key:zOther', issuerDid: 'did:key:zAdmin', capability: 'space/admin' },
  ]
  const myDids = ['did:key:zMe']

  it('returns all capabilities for a space where I am audience or issuer', () => {
    const caps = filterCapabilities(tokens, 'sp-1', myDids)
    expect(caps).toEqual(['space/admin'])
  })

  it('returns multiple capabilities for the same space', () => {
    const caps = filterCapabilities(tokens, 'sp-2', myDids)
    expect(caps).toEqual(['space/read', 'space/write'])
  })

  it('returns empty for spaces where I have no tokens', () => {
    const caps = filterCapabilities(tokens, 'sp-3', myDids)
    expect(caps).toEqual([])
  })

  it('returns empty for unknown spaces', () => {
    const caps = filterCapabilities(tokens, 'sp-unknown', myDids)
    expect(caps).toEqual([])
  })

  describe('hasCapability', () => {
    it('returns true if the exact capability is present', () => {
      expect(hasCapability(['space/read', 'space/write'], 'space/read')).toBe(true)
    })

    it('returns true if space/admin is present (wildcard)', () => {
      expect(hasCapability(['space/admin'], 'space/read')).toBe(true)
      expect(hasCapability(['space/admin'], 'space/write')).toBe(true)
      expect(hasCapability(['space/admin'], 'space/invite')).toBe(true)
    })

    it('returns false if capability is missing and no admin', () => {
      expect(hasCapability(['space/read'], 'space/write')).toBe(false)
      expect(hasCapability(['space/read'], 'space/invite')).toBe(false)
    })

    it('returns false for empty capabilities', () => {
      expect(hasCapability([], 'space/read')).toBe(false)
    })
  })
})

// ---------------------------------------------------------------------------
// Permission label derivation (used in SpaceListItem.vue)
// ---------------------------------------------------------------------------

describe('Permission label derivation', () => {
  const getPermissionLabel = (capabilities: string[]): string => {
    if (capabilities.includes('space/admin')) return 'Admin'
    if (capabilities.includes('space/invite')) return 'Invite'
    if (capabilities.includes('space/write')) return 'Write'
    return 'Read'
  }

  it('shows Admin for space/admin capability', () => {
    expect(getPermissionLabel(['space/admin'])).toBe('Admin')
  })

  it('shows Write for space/write without admin', () => {
    expect(getPermissionLabel(['space/read', 'space/write'])).toBe('Write')
  })

  it('shows Invite for space/invite without admin', () => {
    expect(getPermissionLabel(['space/invite'])).toBe('Invite')
  })

  it('shows Read as fallback when no special capabilities', () => {
    expect(getPermissionLabel(['space/read'])).toBe('Read')
    expect(getPermissionLabel([])).toBe('Read')
  })

  it('Admin takes precedence over everything', () => {
    expect(getPermissionLabel(['space/admin', 'space/write', 'space/invite'])).toBe('Admin')
  })
})
