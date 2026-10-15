// Part of `pnpm check:files` (spec 044, T077): the file permissions of agents as the settings show
// them, with the defaults of a missing row.
import assert from 'node:assert/strict'
import { test } from 'node:test'

import {
  agentFiles,
  BUILTIN_AGENT,
  type PermissionRow,
} from '../src/lib/files/agentPermissions.ts'

const STORAGES = [
  { id: 's1', name: 'Fotos' },
  { id: 's2', name: 'Archiv' },
]

function row(
  agentId: string,
  kind: PermissionRow['kind'],
  target: string,
  status: PermissionRow['status'],
): PermissionRow {
  return { agentId, kind, target, status }
}

test('without rows the built-in agent reaches the device and every storage asks', () => {
  assert.deepEqual(agentFiles([], STORAGES), [
    {
      agentId: BUILTIN_AGENT,
      device: true,
      storages: [
        { id: 's1', name: 'Fotos', status: 'ask' },
        { id: 's2', name: 'Archiv', status: 'ask' },
      ],
      contacts: 'ask',
      contactGroups: [],
      folders: [],
    },
  ])
})

test('rows decide device and storages of their agent', () => {
  const [builtin] = agentFiles(
    [
      row(BUILTIN_AGENT, 'device', '', 'denied'),
      row(BUILTIN_AGENT, 'storage', 's1', 'readWrite'),
      row(BUILTIN_AGENT, 'storage', 's2', 'denied'),
    ],
    STORAGES,
  )
  assert.equal(builtin?.device, false)
  assert.deepEqual(
    builtin?.storages.map((storage) => storage.status),
    ['readWrite', 'denied'],
  )
})

test('other agents follow the built-in one and reach the device only when granted', () => {
  const agents = agentFiles(
    [
      row('zed', 'storage', 's1', 'read'),
      row('alpha', 'device', '', 'granted'),
    ],
    STORAGES,
  )
  assert.deepEqual(
    agents.map((agent) => [agent.agentId, agent.device]),
    [
      [BUILTIN_AGENT, true],
      ['alpha', true],
      ['zed', false],
    ],
  )
  assert.equal(agents[2]?.storages[0]?.status, 'read')
})

test('rows of a storage that is gone are left out', () => {
  const [builtin] = agentFiles(
    [row(BUILTIN_AGENT, 'storage', 'gone', 'read')],
    STORAGES,
  )
  assert.deepEqual(
    builtin?.storages.map((storage) => storage.id),
    ['s1', 's2'],
  )
})

test('remembered folder answers list with name, storage and reach; the device first, a gone storage drops them', () => {
  const [builtin] = agentFiles(
    [
      {
        ...row(BUILTIN_AGENT, 'encryptedFolder', 's2/abc', 'read'),
        reach: 'cloud',
        folderName: 'Unterlagen',
      },
      row(BUILTIN_AGENT, 'encryptedFolder', 's1/def', 'readWrite'),
      row(BUILTIN_AGENT, 'encryptedFolder', 'gone/xyz', 'read'),
      {
        ...row(BUILTIN_AGENT, 'encryptedFolder', 'device:/ghi', 'read'),
        folderName: 'Tresor',
      },
    ],
    STORAGES,
  )
  assert.deepEqual(builtin?.folders, [
    {
      target: 'device:/ghi',
      name: 'Tresor',
      storageName: '',
      onDevice: true,
      status: 'read',
      reach: 'local',
    },
    {
      target: 's2/abc',
      name: 'Unterlagen',
      storageName: 'Archiv',
      onDevice: false,
      status: 'read',
      reach: 'cloud',
    },
    {
      target: 's1/def',
      name: null,
      storageName: 'Fotos',
      onDevice: false,
      status: 'readWrite',
      reach: 'local',
    },
  ])
})

test('the contacts ask without a row; remembered answers for all and for groups list apart', () => {
  const [builtin] = agentFiles(
    [
      row(BUILTIN_AGENT, 'contacts', '*', 'read'),
      row(BUILTIN_AGENT, 'contacts', 'group:g1', 'readWrite'),
      row(BUILTIN_AGENT, 'contacts', 'group:g2', 'denied'),
      row(BUILTIN_AGENT, 'contacts', 'group:', 'read'),
    ],
    STORAGES,
  )
  assert.equal(builtin!.contacts, 'read')
  assert.deepEqual(builtin!.contactGroups, [
    { target: 'group:g1', groupId: 'g1', status: 'readWrite' },
    { target: 'group:g2', groupId: 'g2', status: 'denied' },
  ])
  assert.deepEqual(builtin!.folders, [])
})
