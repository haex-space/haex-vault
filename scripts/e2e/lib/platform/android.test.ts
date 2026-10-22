import { describe, it } from 'node:test'
import assert from 'node:assert/strict'
import { mkdtempSync, readFileSync, readdirSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { createAdb, deviceSerial } from './adb.ts'
import type { AdbRunner } from './adb.ts'
import {
  AndroidData,
  closeVault,
  crashLines,
  frontPackage,
  notificationTitles,
  PACKAGE,
  createAndroidHost,
  mapAndroidUrl,
  prepareDevice,
} from './android.ts'
import { LinuxData, VAULT_DIR } from './linux.ts'

/** An adb that records every call and answers from `answers` (by the joined argument list). */
function fakeAdb(answers: Record<string, string | Error> = {}) {
  const calls: string[] = []
  const run: AdbRunner = (args) => {
    const line = args.join(' ')
    calls.push(line)
    for (const [prefix, answer] of Object.entries(answers)) {
      if (line.startsWith(prefix)) {
        if (answer instanceof Error) throw answer
        return Buffer.from(answer)
      }
    }
    return Buffer.from('')
  }
  return { calls, run }
}

describe('createAdb', () => {
  it('addresses the one device by serial and reads the pid of the app', () => {
    const fake = fakeAdb({ '-s emu shell pidof': '4321\n' })
    const adb = createAdb('emu', fake.run)
    assert.equal(adb.pidof(PACKAGE), 4321)
    assert.deepEqual(fake.calls, [`-s emu shell pidof ${PACKAGE}`])
  })

  it('reads no pid when pidof finds nothing', () => {
    const fake = fakeAdb({ '-s emu shell pidof': new Error('exit 1') })
    assert.equal(createAdb('emu', fake.run).pidof(PACKAGE), undefined)
  })

  it('sees the web view open for debugging by its DevTools socket', () => {
    const fake = fakeAdb({
      '-s emu shell cat /proc/net/unix':
        '00: 0001 @webview_devtools_remote_77\n00: 0001 @other\n',
    })
    const adb = createAdb('emu', fake.run)
    assert.equal(adb.devtoolsOpen(77), true)
    assert.equal(adb.devtoolsOpen(78), false)
  })

  it('makes a port of this machine reachable from the device under the same number', () => {
    const fake = fakeAdb()
    createAdb('emu', fake.run).reverse(4711)
    assert.deepEqual(fake.calls, ['-s emu reverse tcp:4711 tcp:4711'])
  })
})

describe('a device adb lost for a moment', () => {
  it('runs a refused command again once the device is back', () => {
    const calls: string[] = []
    let refused = false
    const run: AdbRunner = (args) => {
      const line = args.join(' ')
      calls.push(line)
      if (line === '-s emu get-state') return Buffer.from('device\n')
      if (!refused) {
        refused = true
        throw new Error(
          'Command failed: adb -s emu shell mkdir -p /x\nadb: device offline',
        )
      }
      return Buffer.from('')
    }
    createAdb('emu', run).shell('mkdir', '-p', '/x')
    assert.deepEqual(calls, [
      '-s emu shell mkdir -p /x',
      '-s emu get-state',
      '-s emu shell mkdir -p /x',
    ])
  })

  it('runs a pull again after any failure, up to three times', () => {
    let pulls = 0
    const calls: string[] = []
    const run: AdbRunner = (args) => {
      const line = args.join(' ')
      calls.push(line)
      if (line === '-s emu get-state') return Buffer.from('device\n')
      pulls += 1
      if (pulls === 1) throw new Error('Command failed: adb -s emu pull /a /b')
      return Buffer.from('')
    }
    createAdb('emu', run).pull('/a', '/b')
    assert.deepEqual(calls, [
      '-s emu pull -Z /a /b',
      '-s emu get-state',
      '-s emu pull -Z /a /b',
    ])
    let twice = 0
    const flaky: AdbRunner = (args) => {
      if (args.join(' ') === '-s emu get-state') return Buffer.from('device\n')
      twice += 1
      if (twice <= 2) throw new Error('Command failed: adb -s emu pull /a /b')
      return Buffer.from('')
    }
    createAdb('emu', flaky).pull('/a', '/b')
    assert.equal(twice, 3)
    const missing = fakeAdb({
      '-s emu get-state': 'device\n',
      '-s emu pull': new Error('remote object does not exist'),
    })
    assert.throws(
      () => createAdb('emu', missing.run).pull('/a', '/b'),
      /does not exist/,
    )
    assert.equal(
      missing.calls.filter((call) => call.includes('pull')).length,
      3,
    )
  })

  it('does not run a command again that failed on the device', () => {
    const fake = fakeAdb({ '-s emu shell false': new Error('exit 1') })
    assert.throws(() => createAdb('emu', fake.run).shell('false'), /exit 1/)
    assert.deepEqual(fake.calls, ['-s emu shell false'])
  })
})

describe('withDevice', () => {
  it('runs the action again once the device is back after adb dropped it', async () => {
    const { withDevice } = await import('./adb.ts')
    const fake = fakeAdb({ '-s emu get-state': 'device\n' })
    const adb = createAdb('emu', fake.run)
    let tries = 0
    const result = withDevice(adb, () => {
      tries += 1
      if (tries === 1) throw new Error('adb: device offline')
      return 'cleared'
    })
    assert.equal(result, 'cleared')
    assert.equal(tries, 2)
    assert.deepEqual(fake.calls, ['-s emu get-state'])
  })

  it('gives up with the first error when the device does not come back', async () => {
    const { withDevice } = await import('./adb.ts')
    const adb = {
      ...createAdb('emu', fakeAdb().run),
      waitForDevice: () => false,
    }
    assert.throws(
      () =>
        withDevice(adb, () => {
          throw new Error('adb: device offline')
        }),
      /device offline/,
    )
  })
})

describe('deviceSerial', () => {
  it('takes ANDROID_SERIAL when set', () => {
    assert.equal(
      deviceSerial({ ANDROID_SERIAL: 'x' }, () => Buffer.from('')),
      'x',
    )
  })

  it('takes the one device adb sees and refuses none or several', () => {
    const list = (body: string) => () =>
      Buffer.from(`List of devices attached\n${body}\n`)
    assert.equal(
      deviceSerial({}, list('emulator-5554\tdevice')),
      'emulator-5554',
    )
    assert.throws(() => deviceSerial({}, list('')), /no device/)
    assert.throws(
      () => deviceSerial({}, list('a\tdevice\nb\tdevice')),
      /2 devices/,
    )
    assert.throws(() => deviceSerial({}, list('a\toffline')), /no device/)
  })
})

describe('prepareDevice', () => {
  it('clears the data, sizes the display like the Linux screen and sets the colour scheme', () => {
    const fake = fakeAdb()
    prepareDevice(createAdb('emu', fake.run), { colorScheme: 'dark' })
    assert.deepEqual(fake.calls, [
      `-s emu shell pm clear ${PACKAGE}`,
      `-s emu shell appops set ${PACKAGE} MANAGE_EXTERNAL_STORAGE allow`,
      '-s emu shell wm size 1280x800',
      '-s emu shell wm density 160',
      '-s emu shell cmd uimode night yes',
    ])
  })

  it('starts without the access to all files when the scenario asks', () => {
    const fake = fakeAdb()
    prepareDevice(createAdb('emu', fake.run), { withoutAllFiles: true })
    assert.ok(
      fake.calls.includes(
        `-s emu shell appops set ${PACKAGE} MANAGE_EXTERNAL_STORAGE default`,
      ),
    )
  })

  it('keeps the data when the scenario reuses it', () => {
    const fake = fakeAdb()
    prepareDevice(createAdb('emu', fake.run), { reuse: true })
    assert.ok(!fake.calls.some((call) => call.includes('pm clear')))
    assert.ok(fake.calls.includes('-s emu shell cmd uimode night no'))
  })
})

describe('frontPackage', () => {
  it('reads the package of the resumed activity', () => {
    const dump = [
      '  Stack #1: type=standard mode=fullscreen',
      '  topResumedActivity=ActivityRecord{6f1a2b u0 com.android.settings/.Settings$AppManageExternalStorageActivity t42}',
      '    mResumedActivity: ActivityRecord{6f1a2b u0 com.android.settings/.Settings$AppManageExternalStorageActivity t42}',
    ].join('\n')
    assert.equal(frontPackage(dump), 'com.android.settings')
    assert.equal(frontPackage('nothing resumed'), undefined)
  })
})

describe('mapAndroidUrl', () => {
  it('maps the desktop app and extension origins to the ones Tauri serves on Android', () => {
    assert.equal(
      mapAndroidUrl('tauri://localhost/closing.html'),
      'http://tauri.localhost/closing.html',
    )
    assert.equal(
      mapAndroidUrl('holzi-ext://localhost/a/b'),
      'http://holzi-ext.localhost/a/b',
    )
    assert.equal(mapAndroidUrl('https://example.org/'), 'https://example.org/')
  })
})

describe('createAndroidHost', () => {
  it('gives one device per scenario and clears its data on dispose', () => {
    const fake = fakeAdb()
    const host = createAndroidHost({
      adb: createAdb('emu', fake.run),
      chromedriver: '/nonexistent/chromedriver',
      marker: 'm',
      logDir: '/tmp',
    })
    const data = host.newData('anna-phone')
    assert.ok(data instanceof AndroidData)
    assert.throws(() => host.newData('anna-laptop'), /one Android device/)
    data.dispose()
    assert.deepEqual(fake.calls, [
      '-s emu shell wm size reset',
      '-s emu shell wm density reset',
      `-s emu shell pm clear ${PACKAGE}`,
    ])
  })
})

describe('reachFromDevice', () => {
  it('reverses the port on an Android run and does nothing otherwise', async () => {
    const { reachFromDevice } = await import('./reach.ts')
    const fake = fakeAdb()
    reachFromDevice(
      5000,
      { E2E_PLATFORM: 'android', ANDROID_SERIAL: 'emu' },
      fake.run,
    )
    reachFromDevice(5001, {}, fake.run)
    assert.deepEqual(fake.calls, ['-s emu reverse tcp:5000 tcp:5000'])
  })

  it("sends the port on through the emulator's network once the preflight prepared it", async () => {
    const { reachFromDevice } = await import('./reach.ts')
    const fake = fakeAdb()
    reachFromDevice(
      5000,
      {
        E2E_PLATFORM: 'android',
        ANDROID_SERIAL: 'emu',
        E2E_ANDROID_REDIRECT: '1',
      },
      fake.run,
    )
    assert.deepEqual(fake.calls, [
      "-s emu shell su 0 sh -c 'iptables -t nat -A holzi_e2e_out -p tcp -d 127.0.0.1 --dport 5000 -j DNAT --to-destination 10.0.2.2:5000'",
    ])
  })
})

describe('redirectWorks', () => {
  it('is true once the device gets the answer of this machine', async () => {
    const { redirectWorks } = await import('./reach.ts')
    const fake = fakeAdb()
    let tries = 0
    const works = await redirectWorks(
      'emu',
      5000,
      async () => (++tries < 2 ? '' : 'holzi-e2e-reach\n'),
      fake.run,
    )
    assert.equal(works, true)
    assert.equal(tries, 2)
    assert.match(fake.calls[0]!, /DNAT --to-destination 10\.0\.2\.2:\d+/)
  })

  it('is false when the device never reaches this machine', async () => {
    const { redirectWorks } = await import('./reach.ts')
    const works = await redirectWorks(
      'emu',
      0,
      async () => {
        throw new Error('nc: connect: Network is unreachable')
      },
      fakeAdb().run,
    )
    assert.equal(works, false)
  })
})

describe('prepareRedirect', () => {
  it('sets up its chains on a device with su', async () => {
    const { prepareRedirect } = await import('./reach.ts')
    const fake = fakeAdb({ "-s emu shell su 0 sh -c 'id -u'": '0\n' })
    assert.equal(prepareRedirect(createAdb('emu', fake.run)), true)
    assert.equal(fake.calls.length, 2)
    assert.match(fake.calls[1]!, /route_localnet=1/)
    assert.match(fake.calls[1]!, /iptables -t nat -F holzi_e2e_out/)
    assert.match(fake.calls[1]!, /-j MASQUERADE/)
  })

  it('leaves a device without su (a phone) to adb reverse', async () => {
    const { prepareRedirect } = await import('./reach.ts')
    const fake = fakeAdb({
      '-s emu shell su': new Error(
        '/system/bin/sh: su: inaccessible or not found',
      ),
    })
    assert.equal(prepareRedirect(createAdb('emu', fake.run)), false)
    assert.equal(fake.calls.length, 1)
  })
})

describe('copying a vault file from the phone (stage 2)', () => {
  const runAs = `-s emu exec-out run-as ${PACKAGE} sh -c`
  const phoneWith = (overrides: Record<string, string | Error> = {}) =>
    fakeAdb({
      [`${runAs} ls instances`]:
        'v.db\nv.db-wal\nv.db.lock\nv.db.vault-id\nother.db\n',
      [`${runAs} cat instances/v.db-wal`]: 'wal',
      [`${runAs} cat instances/v.db.vault-id`]: 'id\n',
      [`${runAs} cat instances/v.db`]: 'vault',
      [`${runAs} stat -c %s instances/v.db-wal`]: '3',
      [`${runAs} stat -c %s instances/v.db.vault-id`]: '3',
      [`${runAs} stat -c %s instances/v.db`]: '5',
      ...overrides,
    })

  it('copies the vault and what it keeps beside it to a Linux device, never the lock', async () => {
    const root = mkdtempSync(join(tmpdir(), 'holzi-android-copy-'))
    try {
      const source = new AndroidData(createAdb('emu', phoneWith().run))
      await source.copyVaultFile('v', new LinuxData(root))
      const target = join(root, ...VAULT_DIR)
      assert.deepEqual(readdirSync(target).sort(), [
        'v.db',
        'v.db-wal',
        'v.db.vault-id',
      ])
      assert.equal(readFileSync(join(target, 'v.db'), 'utf8'), 'vault')
    } finally {
      rmSync(root, { recursive: true, force: true })
    }
  })

  it('leaves nothing behind when a file arrives only in part', async () => {
    const root = mkdtempSync(join(tmpdir(), 'holzi-android-copy-'))
    try {
      const fake = phoneWith({
        [`${runAs} stat -c %s instances/v.db.vault-id`]: '9',
        '-s emu get-state': 'device\n',
      })
      const source = new AndroidData(createAdb('emu', fake.run))
      await assert.rejects(
        source.copyVaultFile('v', new LinuxData(root)),
        /v\.db\.vault-id was read only in part/,
      )
      assert.deepEqual(readdirSync(join(root, ...VAULT_DIR)), [])
      // Read three times before it gives up.
      assert.equal(
        fake.calls.filter((call) =>
          call.endsWith('cat instances/v.db.vault-id'),
        ).length,
        3,
      )
    } finally {
      rmSync(root, { recursive: true, force: true })
    }
  })

  it('reads a file again when adb lost the device in the middle of the transfer', async () => {
    const root = mkdtempSync(join(tmpdir(), 'holzi-android-copy-'))
    try {
      const phone = phoneWith({ '-s emu get-state': 'device\n' })
      let cut = true
      const run: AdbRunner = (args, input) => {
        if (cut && args.join(' ').endsWith('cat instances/v.db')) {
          cut = false
          return Buffer.from('va') // what arrived before the connection broke
        }
        return phone.run(args, input)
      }
      await new AndroidData(createAdb('emu', run)).copyVaultFile(
        'v',
        new LinuxData(root),
      )
      assert.equal(
        readFileSync(join(root, ...VAULT_DIR, 'v.db'), 'utf8'),
        'vault',
      )
    } finally {
      rmSync(root, { recursive: true, force: true })
    }
  })

  it('refuses a running source and another phone as the target', async () => {
    const source = new AndroidData(createAdb('emu', phoneWith().run))
    await assert.rejects(
      source.copyVaultFile(
        'v',
        new AndroidData(createAdb('emu', phoneWith().run)),
      ),
      /Linux device only/,
    )
    source.running = true
    await assert.rejects(
      source.copyVaultFile('v', new LinuxData('/nowhere')),
      /still running/,
    )
  })
})

describe('stopping the phone of a group (stage 2)', () => {
  it('closes the vault and waits for the app to end', async () => {
    let running = true
    const calls: string[] = []
    await closeVault(
      {
        pid: 7,
        invoke: async (command: string) => {
          calls.push(command)
          setTimeout(() => (running = false), 30)
          return { ok: true, data: null }
        },
      } as never,
      { pidof: () => (running ? 7 : undefined) },
    )
    assert.deepEqual(calls, ['close_instance'])
    assert.equal(running, false)
  })

  it('goes on at once when no vault is open', async () => {
    const started = Date.now()
    await closeVault(
      {
        pid: 7,
        invoke: async () => ({ ok: false, error: 'no vault' }),
      } as never,
      { pidof: () => 7 },
      5_000,
    )
    assert.ok(Date.now() - started < 1_000)
  })
})

describe('the notifications of the app (stage 3)', () => {
  it('reads the titles of the notifications of the app only', () => {
    const dump = `
  NotificationRecord(0x1: pkg=com.android.systemui user=UserHandle{0} id=1
      android.title=String (USB debugging connected)
  NotificationRecord(0x2: pkg=${PACKAGE} user=UserHandle{0} id=7
      android.title=String (Standup)
      android.text=String (in 5 Minuten)
`
    assert.deepEqual(notificationTitles(dump), ['Standup'])
  })
})

describe('the crash log of the app', () => {
  it('keeps the entries and drops the section headers', () => {
    const dump = `--------- beginning of crash
10-14 01:02:47.405  3158  3172 F libc    : FORTIFY: pthread_mutex_lock called on a destroyed mutex (0x7efbf8e04880)

`
    assert.deepEqual(crashLines(dump), [
      '10-14 01:02:47.405  3158  3172 F libc    : FORTIFY: pthread_mutex_lock called on a destroyed mutex (0x7efbf8e04880)',
    ])
    assert.deepEqual(crashLines('--------- beginning of crash\n'), [])
    assert.deepEqual(crashLines(''), [])
  })
})
