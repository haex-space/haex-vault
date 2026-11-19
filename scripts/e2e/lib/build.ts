// The application under test: which binary, how it behaves on close, and the debug build the suite
// makes when it is not given one (FR-004). It never builds a release profile itself.
import { accessSync, constants, existsSync, readFileSync } from 'node:fs'
import { isAbsolute, join, resolve } from 'node:path'
import { MARKER_ENV, spawnMarked } from './processes.ts'
import type { CloseBehavior } from './scenario.ts'

export interface ApplicationInfo {
  path: string
  source: 'built' | 'given'
  closeBehavior: CloseBehavior
  closeBehaviorFrom: 'flag' | 'path'
}

export class BuildError extends Error {
  constructor(message: string) {
    super(message)
    this.name = 'BuildError'
  }
}

/** Where cargo puts its output. Cargo runs in `src-tauri`, so a relative directory is relative to it. */
export function targetDirectory(
  repoRoot: string,
  env: NodeJS.ProcessEnv,
): string {
  const configured = env.CARGO_TARGET_DIR
  if (configured === undefined || configured === '')
    return join(repoRoot, 'src-tauri', 'target')
  return isAbsolute(configured)
    ? configured
    : resolve(repoRoot, 'src-tauri', configured)
}

export function debugBinaryPath(
  repoRoot: string,
  env: NodeJS.ProcessEnv,
): string {
  return join(targetDirectory(repoRoot, env), 'debug', 'haex')
}

/**
 * What the application does when its vault is closed: a debug build exits and a release build
 * relaunches (`vault_gate::close_policy`). The path says which, unless it says neither or both, and an
 * explicit value that contradicts the path is rejected rather than trusted.
 */
export function classifyCloseBehavior(
  path: string,
  flag?: CloseBehavior,
): { closeBehavior: CloseBehavior; from: 'flag' | 'path' } {
  const segments = path.split('/')
  const debug = segments.includes('debug')
  const release = segments.includes('release')
  const fromPath: CloseBehavior | undefined =
    debug === release ? undefined : debug ? 'exit' : 'relaunch'
  if (flag !== undefined) {
    if (fromPath !== undefined && fromPath !== flag) {
      throw new Error(
        `--close-behavior ${flag} conflicts with ${path}: a ${debug ? 'debug' : 'release'} build ${fromPath === 'exit' ? 'exits' : 'relaunches'} on close`,
      )
    }
    return { closeBehavior: flag, from: 'flag' }
  }
  if (fromPath === undefined) {
    throw new Error(
      `cannot tell from ${path} whether the application exits or relaunches on close; state it with --close-behavior exit or --close-behavior relaunch`,
    )
  }
  return { closeBehavior: fromPath, from: 'path' }
}

export interface ResolveOptions {
  repoRoot: string
  env: NodeJS.ProcessEnv
  /** A built application, from `--app` or `E2E_APP`. Without it the suite builds the debug application. */
  appPath?: string
  closeBehaviorFlag?: CloseBehavior
}

export function resolveApplication(options: ResolveOptions): ApplicationInfo {
  if (options.appPath === undefined) {
    const path = debugBinaryPath(options.repoRoot, options.env)
    const { closeBehavior, from } = classifyCloseBehavior(
      path,
      options.closeBehaviorFlag,
    )
    return { path, source: 'built', closeBehavior, closeBehaviorFrom: from }
  }
  const path = resolve(options.appPath)
  if (!existsSync(path))
    throw new Error(`the application ${path} does not exist`)
  try {
    accessSync(path, constants.X_OK)
  } catch {
    throw new Error(`the application ${path} is not executable`)
  }
  const { closeBehavior, from } = classifyCloseBehavior(
    path,
    options.closeBehaviorFlag,
  )
  return { path, source: 'given', closeBehavior, closeBehaviorFrom: from }
}

export type BuildRunner = (
  command: string,
  args: string[],
  options: {
    cwd: string
    env: Record<string, string | undefined>
    logFile: string
  },
) => Promise<number>

const runWithLog: BuildRunner = async (command, args, options) => {
  const { child, closed } = spawnMarked(command, args, options)
  const code = await new Promise<number>((resolveCode) => {
    child.once('error', () => resolveCode(127))
    child.once('close', (exit, signal) =>
      resolveCode(exit ?? (signal === null ? 1 : 128)),
    )
  })
  await closed
  return code
}

function tail(file: string, lines: number): string {
  try {
    return readFileSync(file, 'utf8')
      .trimEnd()
      .split('\n')
      .slice(-lines)
      .join('\n')
  } catch {
    return '(no output)'
  }
}

/** Build the debug application with its interface embedded, the way a developer already does. */
export async function buildDebugApplication(options: {
  repoRoot: string
  logFile: string
  marker: string
  run?: BuildRunner
}): Promise<void> {
  const run = options.run ?? runWithLog
  const code = await run('pnpm', ['tauri', 'build', '--debug', '--no-bundle'], {
    cwd: options.repoRoot,
    env: { ...process.env, [MARKER_ENV]: options.marker },
    logFile: options.logFile,
  })
  if (code !== 0) {
    throw new BuildError(
      `the build failed with exit status ${code}; the last 40 lines of ${options.logFile}:\n${tail(options.logFile, 40)}`,
    )
  }
}
