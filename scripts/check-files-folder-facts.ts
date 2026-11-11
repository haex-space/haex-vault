import assert from 'node:assert/strict'
import { test } from 'node:test'
import { computed, effectScope, ref, watch } from 'vue'
import {
  composablePath,
  flush,
  runComposable,
} from './lib/chat-state-harness.ts'
import * as clipboard from '../src/lib/files/clipboard.ts'
import * as encrypted from '../src/lib/files/encrypted.ts'
import * as menus from '../src/lib/files/menus.ts'
import * as registry from '../src/lib/files/registry.ts'
import * as syncRules from '../src/lib/files/syncRules.ts'
import type { SourceRef } from '../src/types/bindings/SourceRef.ts'

test('open-folder sync rules require current, successful and eligible folder facts', async (t) => {
  const plain = {
    name: 'folder',
    path: '/folder',
    kind: 'dir',
    holziOwned: false,
    symlink: false,
    noAccess: false,
    encrypted: null as null | { state: string; foreign: number },
  }
  const pending: Array<{
    resolve: (entry: typeof plain) => void
    reject: (error: Error) => void
  }> = []
  const files = {
    statAsync: () =>
      new Promise<typeof plain>((resolve, reject) => {
        pending.push({ resolve, reject })
      }),
  }
  const imports = {
    '~/composables/useFiles': { asFilesError: () => null },
    '~/lib/files/clipboard': clipboard,
    '~/lib/files/encrypted': encrypted,
    '~/lib/files/menus': menus,
    '~/lib/files/registry': registry,
    '~/lib/files/syncRules': syncRules,
    'vue-sonner': { toast: { error: assert.fail } },
    '@tauri-apps/api/core': { invoke: assert.fail },
  }
  const requireModule = (name: string) => {
    assert.ok(Object.hasOwn(imports, name), `unexpected import: ${name}`)
    return imports[name as keyof typeof imports]
  }
  const globals = { ref, computed, watch, useFiles: () => files }
  const { useFilesFolderFacts } = runComposable(
    composablePath('useFilesFolderFacts'),
    requireModule,
    globals,
  )
  const { useFilesManage } = runComposable(
    composablePath('useFilesManage'),
    requireModule,
    {
      ...globals,
      useFilesFolderFacts,
      useFilesTransfersStore: () => ({ clipboard: null }),
      useDeviceCapabilities: () => ({ isAndroid: ref(false) }),
      useFilesOpenSystem: () => ({}),
    },
  )
  const source = ref<SourceRef>({ kind: 'device' })
  const path = ref<string | null>('/folder')
  const scope = effectScope()
  t.after(() => scope.stop())
  const manage = scope.run(() =>
    useFilesManage({
      source,
      path,
      shown: ref([]),
      tabId: 'test',
      reload: assert.fail,
      activate: assert.fail,
      syncRule: assert.fail,
    }),
  ) as {
    menuEntries: { value: menus.FilesMenuEntry[] }
    folderOwned: { value: boolean }
    folderForeign: { value: number }
  }
  const offered = () =>
    manage.menuEntries.value.some(
      (entry) => 'id' in entry && entry.id === 'syncRule',
    )

  assert.equal(offered(), false, 'pending facts must not offer a rule')
  pending.shift()!.resolve(plain)
  await flush()
  assert.equal(
    offered(),
    true,
    'a successfully loaded plain folder is eligible',
  )

  for (const restricted of [
    { ...plain, holziOwned: true },
    { ...plain, symlink: true },
    { ...plain, noAccess: true },
    { ...plain, encrypted: { state: 'otherVault', foreign: 0 } },
  ]) {
    path.value = `${path.value}/child`
    assert.equal(
      offered(),
      false,
      'navigation immediately invalidates old facts',
    )
    pending.shift()!.resolve(restricted)
    await flush()
    assert.equal(offered(), false, 'restricted folders stay ineligible')
  }

  path.value = '/failed'
  pending.shift()!.reject(new Error('stat failed'))
  await flush()
  assert.equal(offered(), false, 'failed facts must not offer a rule')

  path.value = '/same'
  const oldSource = pending.shift()!
  source.value = { kind: 'storage', storageId: 'cloud' }
  const newSource = pending.shift()!
  newSource.resolve({ ...plain, encrypted: { state: 'readable', foreign: 3 } })
  await flush()
  assert.equal(offered(), true)
  oldSource.resolve({ ...plain, holziOwned: true })
  await flush()
  assert.equal(
    offered(),
    true,
    'same-path result from the previous source is ignored',
  )
  assert.equal(manage.folderOwned.value, false)
  assert.equal(manage.folderForeign.value, 3)

  path.value = '/away'
  const away = pending.shift()!
  path.value = '/same'
  const firstVisit = pending.shift()!
  path.value = '/away'
  const secondAway = pending.shift()!
  path.value = '/same'
  const secondVisit = pending.shift()!
  firstVisit.resolve(plain)
  away.resolve(plain)
  secondAway.reject(new Error('stale failure'))
  await flush()
  assert.equal(
    offered(),
    false,
    'an earlier visit cannot satisfy the current request',
  )
  secondVisit.resolve({ ...plain, holziOwned: true })
  await flush()
  assert.equal(offered(), false)
  assert.equal(manage.folderOwned.value, true)

  path.value = '/leaving'
  const leaving = pending.shift()!
  path.value = null
  leaving.resolve(plain)
  await flush()
  assert.equal(offered(), false, 'the source overview has no open folder')
  assert.equal(manage.folderOwned.value, false)

  path.value = '/disposed'
  const disposed = pending.shift()!
  scope.stop()
  disposed.resolve({ ...plain, holziOwned: true })
  await flush()
  assert.equal(
    manage.folderOwned.value,
    false,
    'unmounted panels discard in-flight results',
  )
})
