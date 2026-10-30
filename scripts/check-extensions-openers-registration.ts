// Part of `pnpm check:extensions` (spec 063, FR-016, research R5): holzi lists itself for `mailto`
// where the system lets a person choose, and never makes itself the program for it.
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { test } from 'node:test'

const read = (path: string) =>
  readFileSync(new URL(`../src-tauri/${path}`, import.meta.url), 'utf8')

interface DeepLinkConfig {
  plugins: {
    'deep-link': {
      desktop: { schemes: string[] }
      mobile?: Array<{ scheme: string[]; host?: string }>
    }
  }
}

const base = JSON.parse(read('tauri.conf.json')) as DeepLinkConfig
const windows = JSON.parse(read('tauri.windows.conf.json')) as DeepLinkConfig

test('the packages of Linux and macOS list holzi for haex-vault and mailto', () => {
  assert.deepEqual(base.plugins['deep-link'].desktop.schemes, [
    'haex-vault',
    'mailto',
  ])
})

test('Android has a filter for mailto next to the contact link', () => {
  const mobile = base.plugins['deep-link'].mobile ?? []
  assert.ok(mobile.some((d) => d.scheme.includes('mailto') && !d.host))
  assert.ok(mobile.some((d) => d.scheme.includes('haex-vault')))
})

test('the Windows installer does not write the class of mailto', () => {
  // The NSIS and WiX templates write `Software\Classes\<scheme>` for every scheme here, which
  // would replace the class of the mail program of the machine.
  assert.deepEqual(windows.plugins['deep-link'].desktop.schemes, ['haex-vault'])
})

test('a build from a directory registers haex-vault only, never every scheme', () => {
  const source = read('src/deep_link/mod.rs')
  const start = source.indexOf('pub fn register_for_development')
  assert.ok(start >= 0, 'register_for_development exists')
  const body = source.slice(start, source.indexOf('\n}\n', start))
  assert.ok(body.includes('register("haex-vault")'))
  assert.ok(!body.includes('register_all'))
})

test('an Android mail intent of the SENDTO action reaches the plugin as a view', () => {
  const manifest = read('gen/android/app/src/main/AndroidManifest.xml')
  // The plugin's block holds the filter for a tapped link (VIEW); the one for SENDTO is ours.
  assert.match(
    manifest,
    /android\.intent\.action\.VIEW[\s\S]*?<data android:scheme="mailto" \/>/,
  )
  assert.match(
    manifest,
    /android\.intent\.action\.SENDTO[\s\S]*?<data android:scheme="mailto" \/>/,
  )
  const activity = read(
    'gen/android/app/src/main/java/com/haex/holzi/MainActivity.kt',
  )
  assert.ok(activity.includes('Intent.ACTION_SENDTO'))
  assert.ok(activity.includes('intent.action = Intent.ACTION_VIEW'))
})
