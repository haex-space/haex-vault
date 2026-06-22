import { readFileSync } from 'node:fs'
import { join } from 'node:path'
import { describe, expect, it } from 'vitest'

/**
 * Security regression guard for the Tauri capability allowlist.
 *
 * `allow-extension-commands` is the IPC surface granted to extension webviews
 * (`ext_*`). Owner-only commands must NEVER appear there: a previous regression
 * exposed `grant_session_permission` / `resolve_permission_prompt`, which let a
 * malicious extension grant itself arbitrary permissions without user consent
 * (and `*_extension_limits`, which let it raise its own sandbox limits).
 *
 * This test fails if any of those commands creeps back into the extension
 * allowlist. It parses the TOML textually (no TOML dependency) by isolating the
 * `allow-extension-commands` block, which ends where `allow-app-commands` begins.
 */
const TOML_PATH = join(
  process.cwd(),
  'src-tauri/permissions/extension-commands.toml',
)

function extensionAllowBlock(): string {
  const toml = readFileSync(TOML_PATH, 'utf-8')
  const extStart = toml.indexOf('identifier = "allow-extension-commands"')
  const appStart = toml.indexOf('identifier = "allow-app-commands"')
  expect(extStart, 'allow-extension-commands block must exist').toBeGreaterThan(-1)
  expect(appStart, 'allow-app-commands block must exist').toBeGreaterThan(extStart)
  return toml.slice(extStart, appStart)
}

// A quoted command literal, e.g. `"grant_session_permission"`. Comments mention
// these names WITHOUT quotes, so this only matches actual allowlist entries.
const quoted = (cmd: string) => `"${cmd}"`

describe('extension-commands.toml allowlist', () => {
  const block = extensionAllowBlock()

  const OWNER_ONLY = [
    // Granting / resolving permission prompts — only the main window may grant.
    'grant_session_permission',
    'resolve_permission_prompt',
    'get_extension_session_permissions',
    'remove_extension_session_permission',
    // Sandbox-limit configuration — owner-only.
    'get_extension_limits',
    'update_extension_limits',
    'reset_extension_limits',
  ]

  it.each(OWNER_ONLY)(
    'must NOT grant owner-only command "%s" to extension webviews',
    (cmd) => {
      expect(block.includes(quoted(cmd))).toBe(false)
    },
  )

  const EXTENSION_FACING = [
    // Extensions may CHECK their own permissions...
    'extension_permissions_check_web',
    'extension_permissions_check_database',
    'extension_permissions_check_filesystem',
    // ...and read/write their OWN logs (identity bound server-side).
    'extension_logging_write',
    'extension_logging_read',
  ]

  it.each(EXTENSION_FACING)(
    'still grants extension-facing command "%s"',
    (cmd) => {
      expect(block.includes(quoted(cmd))).toBe(true)
    },
  )
})
