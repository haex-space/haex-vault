import assert from 'node:assert/strict'
import { spawnSync } from 'node:child_process'
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join, resolve } from 'node:path'
import { test } from 'node:test'

const bridge = resolve('scripts/with-nix-host-bridge.sh')
const bash = spawnSync('which', ['bash'], { encoding: 'utf8' }).stdout.trim()

for (const scenario of [
  {
    name: 'multiarch with newer Nix libc',
    host: '2.39',
    nix: '2.44',
    multiarch: true,
    useNix: true,
  },
  { name: 'standard layout with newer host libc', host: '2.44', nix: '2.39' },
  { name: 'numeric libc ordering', host: '2.9', nix: '2.44', useNix: true },
  {
    name: 'missing loader fails clearly',
    host: '2.39',
    nix: '2.44',
    noLoader: true,
  },
  {
    name: 'empty metadata libdir fails clearly',
    host: '2.44',
    nix: '2.39',
    emptyLibdir: true,
  },
  {
    name: 'empty inherited metadata path',
    host: '2.44',
    nix: '2.39',
    emptyPath: true,
  },
  { name: 'equal libc versions prefer host', host: '2.44', nix: '2.44' },
  {
    name: 'explicit display backend is preserved',
    host: '2.44',
    nix: '2.39',
    backend: 'wayland',
  },
  {
    name: 'missing host metadata fails clearly',
    host: '2.44',
    nix: '2.39',
    missing: true,
  },
  { name: 'Linux outside Nix bypasses discovery', bypass: true },
  { name: 'non-Linux bypasses discovery', platform: 'Darwin' },
]) {
  test(scenario.name, (t) => {
    const root = mkdtempSync(join(tmpdir(), 'holzi bridge '))
    t.after(() => rmSync(root, { recursive: true, force: true }))
    const bin = join(root, 'bin')
    const hostBin = join(root, 'host-bin')
    const hostLib = join(
      root,
      scenario.multiarch ? 'lib/x86_64-linux-gnu' : 'lib',
    )
    const nixLib = join(root, 'nix-lib')
    for (const dir of [bin, hostBin, hostLib, nixLib])
      mkdirSync(dir, { recursive: true })
    for (const dir of [hostLib, nixLib]) {
      writeFileSync(join(dir, 'libc.so.6'), '')
      if (!scenario.noLoader)
        writeFileSync(join(dir, 'ld-linux-x86-64.so.2'), '')
    }
    const cache = join(hostLib, 'loaders.cache')
    writeFileSync(cache, 'host pixbuf cache')
    const env = {
      ...process.env,
      PATH: `${bin}:${process.env.PATH}`,
      IN_NIX_SHELL: scenario.bypass ? '' : 'impure',
      FIXTURE_HOST_BIN: hostBin,
      FIXTURE_HOST_LIB: hostLib,
      FIXTURE_NIX_LIB: nixLib,
      FIXTURE_HOST_VERSION: scenario.host ?? '2.44',
      FIXTURE_NIX_VERSION: scenario.nix ?? '2.39',
      FIXTURE_PLATFORM: scenario.platform ?? 'Linux',
      FIXTURE_MISSING: scenario.missing ? '1' : '',
      FIXTURE_EMPTY_LIBDIR: scenario.emptyLibdir ? '1' : '',
      PKG_CONFIG_PATH: scenario.emptyPath ? '' : '/fixture/nix/pkgconfig',
      HOLZI_NIX_DYNAMIC_LINKER: undefined,
      HOLZI_NIX_RUNTIME_LIBRARY_PATH: undefined,
      PKG_CONFIG_LIBDIR: '/fixture/nix/libdir',
      LD_LIBRARY_PATH: process.env.LD_LIBRARY_PATH ?? '',
      GDK_PIXBUF_MODULE_FILE: '/fixture/nix/loaders.cache',
      GDK_BACKEND: scenario.backend,
      GTK_CSD: undefined,
      XDG_CACHE_HOME: root,
    }
    for (const [path, body] of [
      [join(bin, 'uname'), 'echo "$FIXTURE_PLATFORM"'],
      [
        join(bin, 'getconf'),
        'if [ "$1" = PATH ]; then echo "$FIXTURE_HOST_BIN"; else echo "glibc $FIXTURE_NIX_VERSION"; fi',
      ],
      [join(hostBin, 'getconf'), 'echo "glibc $FIXTURE_HOST_VERSION"'],
      [
        join(hostBin, 'pkg-config'),
        'test -z "${LD_LIBRARY_PATH:-}${PKG_CONFIG_PATH:-}${PKG_CONFIG_LIBDIR:-}" || exit 90\necho "$FIXTURE_HOST_LIB/pkgconfig"',
      ],
      [
        join(bin, 'pkg-config'),
        'test -z "$FIXTURE_MISSING" || exit 1\ncase "$1" in\n--exists) case "$PKG_CONFIG_PATH" in "$FIXTURE_HOST_LIB/pkgconfig"*) exit 0;; *) exit 1;; esac;;\n--variable=libdir) test -z "$FIXTURE_EMPTY_LIBDIR" || exit 0; echo "$FIXTURE_HOST_LIB";;\n--variable=gdk_pixbuf_cache_file) echo "$FIXTURE_HOST_LIB/loaders.cache";;\nesac',
      ],
      [join(bin, 'readlink'), 'echo "$FIXTURE_HOST_LIB/ld-linux-x86-64.so.2"'],
      [join(bin, 'cc'), 'echo "$FIXTURE_NIX_LIB/${1#-print-file-name=}"'],
      [join(bin, 'gdk-pixbuf-query-loaders'), 'exit 0'],
    ]) {
      writeFileSync(path!, `#!${bash}\nset -eu\n${body}\n`, { mode: 0o755 })
    }
    const result = spawnSync(
      bash,
      [
        bridge,
        process.execPath,
        '-e',
        'console.log(JSON.stringify(process.env)); process.exit(Number(process.argv[1] ?? 0))',
        '0',
      ],
      { env, encoding: 'utf8' },
    )
    if (scenario.noLoader || scenario.emptyLibdir) {
      assert.notEqual(result.status, 0)
      assert.match(
        result.stderr,
        scenario.noLoader ? /matching.*libc/ : /no usable library directory/,
      )
      return
    }
    if (scenario.missing) {
      assert.notEqual(result.status, 0)
      assert.match(result.stderr, /host.*(WebKit|webkit|metadata)/i)
      return
    }
    assert.equal(result.status, 0, result.stderr)
    const actual = JSON.parse(result.stdout) as NodeJS.ProcessEnv
    assert.equal(actual.LD_LIBRARY_PATH, env.LD_LIBRARY_PATH)
    assert.equal(actual.GDK_BACKEND, env.GDK_BACKEND)
    assert.equal(actual.GTK_CSD, undefined)
    if (scenario.bypass || scenario.platform) {
      assert.equal(actual.PKG_CONFIG_PATH, env.PKG_CONFIG_PATH)
      assert.equal(actual.HOLZI_NIX_DYNAMIC_LINKER, undefined)
      return
    }
    const selectedLib = scenario.useNix ? nixLib : hostLib
    assert.equal(
      actual.HOLZI_NIX_DYNAMIC_LINKER,
      join(selectedLib, 'ld-linux-x86-64.so.2'),
    )
    assert.equal(
      actual.HOLZI_NIX_RUNTIME_LIBRARY_PATH,
      [...new Set([selectedLib, hostLib])].join(':'),
    )
    assert.equal(
      actual.PKG_CONFIG_PATH,
      `${hostLib}/pkgconfig${env.PKG_CONFIG_PATH ? `:${env.PKG_CONFIG_PATH}` : ''}`,
    )
    assert.equal(actual.GDK_PIXBUF_MODULE_FILE, cache)
    const failedChild = spawnSync(
      bash,
      [bridge, process.execPath, '-e', 'process.exit(17)'],
      { env, encoding: 'utf8' },
    )
    assert.equal(failedChild.status, 17, failedChild.stderr)
  })
}
