// `pnpm check:android-versions` (spec 043, research R15): the CI sets up its own Android
// environment instead of the Nix devShell, so this check keeps both on the same versions. The
// devShell is the source: `.devshell/packages.nix` (SDK platforms, build tools, NDK) and
// `.devshell/rust-toolchain.toml` (Rust channel and Android targets), both delivered by the atoms
// molecule `haex-vault`. Every workflow that installs SDK packages must name exactly the devShell's
// packages, the same Rust channel, and only Android targets the devShell has.
import assert from 'node:assert/strict'
import { existsSync, readFileSync } from 'node:fs'
import { test } from 'node:test'

export interface AndroidVersions {
  platforms: string[]
  buildTools: string[]
  ndk: string[]
  rustChannel: string | null
  rustTargets: string[]
}

const sorted = (values: Iterable<string>): string[] =>
  [...new Set(values)].sort()

const quotedList = (text: string, name: string): string[] => {
  const list = new RegExp(`${name}\\s*=\\s*\\[([^\\]]*)\\]`).exec(text)
  return list ? [...list[1]!.matchAll(/"([^"]+)"/g)].map((m) => m[1]!) : []
}

/** The versions the devShell installs. */
export function fromDevshell(
  packagesNix: string,
  toolchainToml: string,
): AndroidVersions {
  const ndkVersion = /ndkVersion\s*=\s*"([^"]+)"/.exec(packagesNix)?.[1]
  return {
    platforms: sorted(quotedList(packagesNix, 'platformVersions')),
    buildTools: sorted(quotedList(packagesNix, 'buildToolsVersions')),
    ndk: ndkVersion ? [ndkVersion] : [],
    rustChannel: /channel\s*=\s*"([^"]+)"/.exec(toolchainToml)?.[1] ?? null,
    rustTargets: sorted(quotedList(toolchainToml, 'targets')),
  }
}

/** The versions a workflow installs; empty lists when it sets up no Android SDK. */
export function fromWorkflow(workflow: string): AndroidVersions {
  const all = (pattern: RegExp): string[] =>
    sorted([...workflow.matchAll(pattern)].map((m) => m[1]!))
  const targets = [...workflow.matchAll(/targets:\s*([^\n#]+)/g)]
    .flatMap((m) => m[1]!.split(/[,\s]+/))
    .filter((target) => target.includes('android'))
  return {
    platforms: all(/"platforms;android-([^"]+)"/g),
    buildTools: all(/"build-tools;([^"]+)"/g),
    ndk: all(/"ndk;([^"]+)"/g),
    rustChannel: /toolchain:\s*['"]?([0-9][0-9.]*)/.exec(workflow)?.[1] ?? null,
    rustTargets: sorted(targets),
  }
}

/** What a workflow gets wrong against the devShell; empty when it matches. */
export function mismatches(
  devshell: AndroidVersions,
  workflow: AndroidVersions,
): string[] {
  const problems: string[] = []
  for (const key of ['platforms', 'buildTools', 'ndk'] as const) {
    if (JSON.stringify(workflow[key]) !== JSON.stringify(devshell[key])) {
      problems.push(
        `${key}: workflow ${workflow[key].join(' ')} ≠ devShell ${devshell[key].join(' ')}`,
      )
    }
  }
  if (workflow.rustChannel !== devshell.rustChannel) {
    problems.push(
      `rust: workflow ${workflow.rustChannel} ≠ devShell ${devshell.rustChannel}`,
    )
  }
  const foreign = workflow.rustTargets.filter(
    (target) => !devshell.rustTargets.includes(target),
  )
  if (foreign.length > 0)
    problems.push(`rust targets not in the devShell: ${foreign.join(' ')}`)
  if (workflow.rustTargets.length === 0) problems.push('no Android rust target')
  return problems
}

const DEVSHELL_NIX = `
  ndkVersion = "28.2.13676358";
  android = composeAndroidPackages {
    platformVersions = [ "36" "37.0" ];
    buildToolsVersions = [ "36.0.0" "37.0.0" ];
  };`
const TOOLCHAIN = `[toolchain]
channel = "1.98.1"
targets = [
  "aarch64-linux-android",
  "x86_64-linux-android",
]`
const WORKFLOW = `
      - uses: dtolnay/rust-toolchain@master
        with:
          toolchain: 1.98.1
          targets: aarch64-linux-android, x86_64-linux-android
      - run: >-
          sdkmanager --install "platforms;android-36" "platforms;android-37.0"
          "build-tools;36.0.0" "build-tools;37.0.0" "ndk;28.2.13676358"`

test('reads the devShell versions', () => {
  assert.deepEqual(fromDevshell(DEVSHELL_NIX, TOOLCHAIN), {
    platforms: ['36', '37.0'],
    buildTools: ['36.0.0', '37.0.0'],
    ndk: ['28.2.13676358'],
    rustChannel: '1.98.1',
    rustTargets: ['aarch64-linux-android', 'x86_64-linux-android'],
  })
})

test('a workflow with the same versions matches', () => {
  assert.deepEqual(
    mismatches(fromDevshell(DEVSHELL_NIX, TOOLCHAIN), fromWorkflow(WORKFLOW)),
    [],
  )
})

test('a different NDK, Rust channel or foreign target is reported', () => {
  const devshell = fromDevshell(DEVSHELL_NIX, TOOLCHAIN)
  const drifted = WORKFLOW.replace('ndk;28.2.13676358', 'ndk;29.0.1')
    .replace('toolchain: 1.98.1', 'toolchain: 1.99.0')
    .replace('x86_64-linux-android', 'riscv64-linux-android')
  const problems = mismatches(devshell, fromWorkflow(drifted))
  assert.equal(problems.length, 3)
  assert.match(problems[0]!, /^ndk:/)
  assert.match(problems[1]!, /^rust:/)
  assert.match(problems[2]!, /riscv64-linux-android/)
})

test('a missing platform is reported', () => {
  const devshell = fromDevshell(DEVSHELL_NIX, TOOLCHAIN)
  const problems = mismatches(
    devshell,
    fromWorkflow(WORKFLOW.replace(' "platforms;android-37.0"', '')),
  )
  assert.deepEqual(problems, ['platforms: workflow 36 ≠ devShell 36 37.0'])
})

test('the CI workflows use the devShell versions', () => {
  const devshell = fromDevshell(
    readFileSync('.devshell/packages.nix', 'utf8'),
    readFileSync('.devshell/rust-toolchain.toml', 'utf8'),
  )
  const workflows = [
    '.github/workflows/ci.yml',
    '.github/workflows/android-release.yml',
  ].filter((path) => existsSync(path))
  const android = workflows.filter((path) =>
    readFileSync(path, 'utf8').includes('sdkmanager'),
  )
  assert.ok(
    android.includes('.github/workflows/ci.yml'),
    'ci.yml sets up no Android SDK (job android-build)',
  )
  for (const path of android) {
    assert.deepEqual(
      mismatches(devshell, fromWorkflow(readFileSync(path, 'utf8'))),
      [],
      path,
    )
  }
})
