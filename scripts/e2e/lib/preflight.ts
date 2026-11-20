// The checks before anything is started: the tools are there, and the driver is the same version as the
// web view the application uses on this machine (FR-011). A driver that differs fails in ways that look
// like application bugs, so this runs first and stops the run with a message that names the remedy.
import { execFile } from 'node:child_process'
import { accessSync, constants, readFileSync, statSync } from 'node:fs'
import { delimiter, dirname, join } from 'node:path'

/** The tools the suite needs from the machine, by the name they are found under. */
export const REQUIRED_TOOLS = [
  'tauri-driver',
  'WebKitWebDriver',
  'xvfb-run',
] as const
export type ToolName = (typeof REQUIRED_TOOLS)[number]

export interface Tools {
  tauriDriver: string
  webKitWebDriver: string
  xvfbRun: string
}

export interface ResolvedTools {
  found: Partial<Record<ToolName, string>>
  missing: ToolName[]
}

export interface ToolFacts {
  name: string
  path?: string
  ok: boolean
  remedy?: string
}

export interface ToolCheck {
  tools: ToolFacts[]
  found: Partial<Record<ToolName, string>>
  driverVersion?: string
  webviewVersion?: string
  result: 'ok' | 'failed'
  messages: string[]
}

export type CommandRunner = (
  command: string,
  args: string[],
  limitMs: number,
) => Promise<{ code: number; stdout: string } | null>

const COMMAND_LIMIT_MS = 5000

function isExecutableFile(path: string): boolean {
  try {
    if (!statSync(path).isFile()) return false
    accessSync(path, constants.X_OK)
    return true
  } catch {
    return false
  }
}

/** Find each required tool on `PATH` without a shell. The path is the one found, not a resolved link. */
export function resolveTools(pathEnv: string | undefined): ResolvedTools {
  const directories = (pathEnv ?? '')
    .split(delimiter)
    .filter((entry) => entry !== '')
  const found: Partial<Record<ToolName, string>> = {}
  const missing: ToolName[] = []
  for (const name of REQUIRED_TOOLS) {
    const hit = directories.map((dir) => join(dir, name)).find(isExecutableFile)
    if (hit === undefined) missing.push(name)
    else found[name] = hit
  }
  return { found, missing }
}

/** The tools as the instance code names them. Only valid when none is missing. */
export function toTools(found: Partial<Record<ToolName, string>>): Tools {
  const need = (name: ToolName): string => {
    const path = found[name]
    if (path === undefined) throw new Error(`${name} is not available`)
    return path
  }
  return {
    tauriDriver: need('tauri-driver'),
    webKitWebDriver: need('WebKitWebDriver'),
    xvfbRun: need('xvfb-run'),
  }
}

/** Run a command with a limit. `null` means it could not start or did not finish in time. */
export const runCommand: CommandRunner = (command, args, limitMs) =>
  new Promise((resolve) => {
    execFile(
      command,
      args,
      { timeout: limitMs, encoding: 'utf8' },
      (error, stdout) => {
        if (error === null) return resolve({ code: 0, stdout })
        const code = (error as NodeJS.ErrnoException & { code?: unknown }).code
        resolve(typeof code === 'number' ? { code, stdout } : null)
      },
    )
  })

/** `2.44.2-0ubuntu0.24.04.1` and `2:2.44.2-1` both become `2.44.2`: the upstream version only. */
export function normalizeVersion(raw: string): string | null {
  return /(\d+\.\d+\.\d+)/.exec(raw.trim().replace(/^\d+:/, ''))?.[1] ?? null
}

const DEV_SHELL =
  'enter the dev shell with `nix develop` (it provides the tool once haex-vault pins the atoms revision that delivers the e2e tooling)'
const REMEDIES: Record<ToolName, string> = {
  'tauri-driver': `${DEV_SHELL}, or install it with \`cargo install tauri-driver --locked --version 2.0.6\``,
  WebKitWebDriver: `${DEV_SHELL}, or install the package that ships it (webkit2gtk-driver on Debian and Ubuntu)`,
  'xvfb-run': `${DEV_SHELL}, or install the xvfb package that ships it`,
}

function readTrimmed(file: string): string | null {
  try {
    return readFileSync(file, 'utf8').trim()
  } catch {
    return null
  }
}

async function driverVersionOf(
  driverPath: string,
  run: CommandRunner,
): Promise<{ version?: string; problem?: string }> {
  // The delivered driver is a link into another package, so the file is looked up next to the path the
  // driver was found under, not next to where the link points.
  const file = join(
    dirname(dirname(driverPath)),
    'share',
    'webkit-webdriver',
    'version',
  )
  const fromFile = readTrimmed(file)
  const version = fromFile === null ? null : normalizeVersion(fromFile)
  if (version !== null) return { version }
  const installed = await run(
    'dpkg-query',
    ['-W', '-f=${Version}', 'webkit2gtk-driver'],
    COMMAND_LIMIT_MS,
  )
  const fromPackage =
    installed !== null && installed.code === 0
      ? normalizeVersion(installed.stdout)
      : null
  if (fromPackage !== null) return { version: fromPackage }
  return {
    problem: `cannot determine the version of ${driverPath}: there is no version file at ${file} and dpkg-query does not know the package webkit2gtk-driver`,
  }
}

export async function checkPreflight(input: {
  pathEnv: string | undefined
  run?: CommandRunner
}): Promise<ToolCheck> {
  const run = input.run ?? runCommand
  const { found, missing } = resolveTools(input.pathEnv)
  const tools: ToolFacts[] = REQUIRED_TOOLS.map((name) =>
    found[name] === undefined
      ? { name, ok: false, remedy: REMEDIES[name] }
      : { name, path: found[name], ok: true },
  )
  const messages = missing.map(
    (name) => `${name} is not on PATH. Remedy: ${REMEDIES[name]}`,
  )

  let driverVersion: string | undefined
  const driver = found.WebKitWebDriver
  if (driver !== undefined) {
    const outcome = await driverVersionOf(driver, run)
    driverVersion = outcome.version
    if (outcome.problem !== undefined) messages.push(outcome.problem)
  }

  let webviewVersion: string | undefined
  const modversion = await run(
    'pkg-config',
    ['--modversion', 'webkit2gtk-4.1'],
    COMMAND_LIMIT_MS,
  )
  const webview =
    modversion !== null && modversion.code === 0
      ? normalizeVersion(modversion.stdout)
      : null
  if (webview === null) {
    messages.push(
      'cannot determine the version of the web view: `pkg-config --modversion webkit2gtk-4.1` failed; install the development files (libwebkit2gtk-4.1-dev) or run through scripts/with-nix-host-bridge.sh, which points PKG_CONFIG_PATH at the host web view',
    )
  } else {
    webviewVersion = webview
  }

  if (
    driverVersion !== undefined &&
    webviewVersion !== undefined &&
    driverVersion !== webviewVersion
  ) {
    messages.push(
      `WebKitWebDriver is ${driverVersion} but the web view the application uses is ${webviewVersion}; the two must be the same version. Remedy: make them agree (update the driver through haex-vault's atoms pin or update the host's web view), then enter the dev shell again with \`nix develop\`; without Nix, install the driver package that matches libwebkit2gtk-4.1 ${webviewVersion}`,
    )
  }

  return {
    tools,
    found,
    driverVersion,
    webviewVersion,
    result: messages.length === 0 ? 'ok' : 'failed',
    messages,
  }
}
