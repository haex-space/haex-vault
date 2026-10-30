// Scenarios that do not run on a platform (spec 043 FR-033b, data-model.md). `excluded` is final: the
// function does not exist there (FR-016) or works differently (FR-006), and a counter case checks the
// "not available" answer. `pending` runs there once the named stage of the plan is done; after stage 5
// none is left (`PENDING_ALLOWED` turns false). `pnpm check:e2e-exclusions` checks these rules.

export type Platform = 'android'

export type PlatformExclusion =
  | {
      scenario: string
      platform: Platform
      kind: 'excluded'
      /** Why, with the requirement (FR-006 or FR-016). */
      reason: string
      /** The scenario that checks the "not available" answer (or the different behaviour) instead. */
      counterCase: string
    }
  | {
      scenario: string
      platform: Platform
      kind: 'pending'
      /** The stage of the plan that makes it run (`2`, `3`, `1c`, …). */
      stage: string
      reason: string
    }

/** False from stage 5 on: every scenario then runs or is excluded for good (SC-007). */
export const PENDING_ALLOWED = false

/** The least share of the desktop scenarios that must run on Android (SC-007). */
export const MINIMUM_COVERAGE = 0.8

export const EXCLUSIONS: PlatformExclusion[] = [
  {
    scenario: 'mcp-agent',
    platform: 'android',
    kind: 'excluded',
    reason:
      'external agents reach holzi through the `--mcp-agent` helper, which a phone does not have (FR-016, spec 021 research R12); the HTTP way comes with spec 021 PR G',
    counterCase: 'android-not-available',
  },
  {
    scenario: 'agent-browser',
    platform: 'android',
    kind: 'excluded',
    reason:
      'a phone has no agent browser: holzi steers a Chromium of its own only on the desktop (FR-016, spec 065 research R13), and its stand-in agent needs the `--mcp-agent` helper',
    counterCase: 'android-not-available',
  },
  {
    scenario: 'files-split-restore',
    platform: 'android',
    kind: 'excluded',
    reason:
      'the two-panel file browser is not offered on a phone (FR-016, spec 062 FR-021)',
    counterCase: 'files-restore',
  },
  {
    scenario: 'files-split-large',
    platform: 'android',
    kind: 'excluded',
    reason:
      'the two-panel file browser is not offered on a phone (FR-016, spec 062 FR-021)',
    counterCase: 'files-large-folder',
  },
  {
    scenario: 'files-split-storage',
    platform: 'android',
    kind: 'excluded',
    reason:
      'the two-panel file browser is not offered on a phone (FR-016, spec 062 FR-021)',
    counterCase: 'files-basic',
  },
  {
    scenario: 'files-split',
    platform: 'android',
    kind: 'excluded',
    reason:
      'the two-panel file browser is not offered on a phone (FR-016, spec 062 FR-021)',
    counterCase: 'files-basic',
  },
  {
    scenario: 'browser-bridge',
    platform: 'android',
    kind: 'excluded',
    reason:
      'the External Bridge for browser extensions is not offered on a phone (FR-016, spec 049 FR-043)',
    counterCase: 'android-not-available',
  },
  {
    scenario: 'extension-files',
    platform: 'android',
    kind: 'excluded',
    reason:
      'free file paths and folder watching do not exist on Android (FR-016)',
    counterCase: 'android-not-available',
  },
  {
    scenario: 'extension-dev-mode',
    platform: 'android',
    kind: 'excluded',
    reason:
      'loads an extension from a project folder, which needs a folder path (FR-016)',
    counterCase: 'android-not-available',
  },
  {
    scenario: 'extension-encrypted-device',
    platform: 'android',
    kind: 'excluded',
    reason:
      'an extension reaches an encrypted folder on the device only by file access to its place, and there are no free paths on Android (FR-016)',
    counterCase: 'android-not-available',
  },
  {
    scenario: 'relaunch-after-lock',
    platform: 'android',
    kind: 'excluded',
    reason:
      'closing the vault ends the app on Android; there is no relaunch (FR-006)',
    counterCase: 'lock-twice',
  },
]

/** The entries of `platform`, by scenario. */
export function exclusionsFor(
  platform: Platform,
  entries: PlatformExclusion[] = EXCLUSIONS,
): Map<string, PlatformExclusion> {
  return new Map(
    entries
      .filter((entry) => entry.platform === platform)
      .map((entry) => [entry.scenario, entry]),
  )
}
