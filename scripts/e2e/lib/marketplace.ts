// A marketplace for the scenarios of spec 069: a small `http` server that speaks the protocol of
// specs/069-marketplace/contracts/marketplace-api.md and serves the signed fixture bundles of
// `src-tauri/tests/fixtures/extension_e2e/`. The app under test learns its address through
// `HOLZI_E2E_MARKETPLACE_URL`, which a debug build reads when it seeds the preset marketplace of a
// new vault. Size and SHA-256 are computed from the file when served; switches make them wrong or
// missing, and every request is recorded.
import { createHash, randomBytes } from 'node:crypto'
import { readFileSync } from 'node:fs'
import { createServer, type IncomingMessage, type Server } from 'node:http'
import type { AddressInfo } from 'node:net'
import { fixture, reachableFromDevice } from './extensions.ts'
import { unwrap } from './flows.ts'
import { resizeAppWindow } from './settings.ts'
import type { FlowInstance } from './flows.ts'

/** The publisher key of the fixture bundles (their manifests name it). */
const FIXTURE_KEY =
  '1549fb64ff115627d507f8c95a8fd7b4bd5381c6df97120f910ed904ba50e63f'

/** The variable a debug build reads the address of its preset marketplace from. */
export const ADDRESS_VARIABLE = 'HOLZI_E2E_MARKETPLACE_URL'

export interface ServedVersion {
  version: string
  /** Where the bytes come from: a `fixture('e2e', …)` path. */
  file: string
  changelog?: string
  minAppVersion?: string
  maxAppVersion?: string
}

export interface ServedExtension {
  /** The slug in the paths; the name of the manifest for the two real ones, so the catalog
   * recognises an installed extension. */
  slug: string
  name: string
  description: string
  category: string
  downloads: number
  /** Newest first, as the live service sends them. */
  versions: ServedVersion[]
}

export type HashSwitch = 'ok' | 'wrong' | 'missing'
export type SizeSwitch = 'ok' | 'wrong'

export interface RecordedRequest {
  path: string
  query: string
  /** The `Authorization` header as it arrived, if any. */
  authorization?: string
}

export interface MarketplaceOptions {
  /** A path prefix such as `/market`, as a self-hosted marketplace may live behind one. */
  prefix?: string
  /** Require `Authorization: Bearer <token>` on every call of the catalog. */
  token?: boolean
  /** Replaces the default two extensions and their fillers. */
  extensions?: ServedExtension[]
}

export interface TestMarketplace {
  /** The address a marketplace row holds (origin plus prefix). */
  readonly url: string
  readonly origin: string
  /** The token when `token` was asked for. */
  readonly token: string | undefined
  readonly requests: RecordedRequest[]
  readonly extensions: ServedExtension[]
  /** What the hash of every version in the answers looks like. */
  hash: HashSwitch
  /** `wrong` promises one byte more than the file has. */
  size: SizeSwitch
  /** Answer every call with 503 until set back. */
  down: boolean
  /** How many requests reached `path` (without the prefix). */
  hits(path: string): number
  close(): Promise<void>
}

const sha256 = (bytes: Buffer): string =>
  createHash('sha256').update(bytes).digest('hex')

/** The two real extensions the scenarios install, and fillers so the catalog has a second page. */
export function defaultExtensions(): ServedExtension[] {
  const real: ServedExtension[] = [
    {
      slug: 'probe',
      name: 'probe',
      description: 'The probe extension of the end-to-end suite.\nSecond line.',
      category: 'tools',
      downloads: 9000,
      versions: [{ version: '1.0.0', file: fixture('e2e', 'probe') }],
    },
    {
      slug: 'agent-tools',
      name: 'agent-tools',
      description: 'The test extension that offers tools to the agent.',
      category: 'tools',
      downloads: 8000,
      versions: [{ version: '1.0.0', file: fixture('e2e', 'agent-tools') }],
    },
  ]
  const fillers = Array.from({ length: 24 }, (_, index) => {
    const number = String(index + 1).padStart(2, '0')
    return {
      slug: `filler-${number}`,
      name: `filler-${number}`,
      description: `Filler ${number}`,
      category: index % 2 === 0 ? 'games' : 'tools',
      downloads: 100 - index,
      versions: [{ version: '1.0.0', file: fixture('e2e', 'probe') }],
    }
  })
  return [...real, ...fillers]
}

const CATEGORY_NAMES: Record<string, string> = {
  tools: 'Tools',
  games: 'Games',
}

/** Starts the marketplace on a free port of the loopback interface. */
export async function startMarketplace(
  options: MarketplaceOptions = {},
): Promise<TestMarketplace> {
  const prefix = (options.prefix ?? '').replace(/\/+$/, '')
  const token = options.token
    ? randomBytes(18).toString('base64url')
    : undefined
  const extensions = options.extensions ?? defaultExtensions()
  const requests: RecordedRequest[] = []
  const state = {
    hash: 'ok' as HashSwitch,
    size: 'ok' as SizeSwitch,
    down: false,
  }
  let origin = ''

  const bytesOf = (version: ServedVersion): Buffer => readFileSync(version.file)

  const versionJson = (version: ServedVersion) => {
    const bytes = bytesOf(version)
    const hash = sha256(bytes)
    return {
      version: version.version,
      changelog: version.changelog ?? null,
      bundleSize: bytes.length + (state.size === 'wrong' ? 1 : 0),
      ...(state.hash === 'missing'
        ? {}
        : {
            bundleHash:
              state.hash === 'wrong'
                ? `${hash[0] === '0' ? '1' : '0'}${hash.slice(1)}`
                : hash,
          }),
      minAppVersion: version.minAppVersion ?? null,
      maxAppVersion: version.maxAppVersion ?? null,
    }
  }

  const entryJson = (extension: ServedExtension) => ({
    extensionId: `e2e/${extension.slug}`,
    name: extension.name,
    slug: extension.slug,
    shortDescription: extension.description.split('\n')[0],
    iconUrl: null,
    verified: false,
    totalDownloads: extension.downloads,
    averageRating: null,
    reviewCount: 0,
    tags: [extension.category],
    publisher: { displayName: 'haex', slug: 'haex', verified: false },
    category: {
      name: CATEGORY_NAMES[extension.category] ?? extension.category,
      slug: extension.category,
    },
    versions: extension.versions.map(versionJson),
  })

  function json(
    res: import('node:http').ServerResponse,
    status: number,
    body: unknown,
  ) {
    res.writeHead(status, { 'content-type': 'application/json' })
    res.end(JSON.stringify(body))
  }

  function handle(
    req: IncomingMessage,
    res: import('node:http').ServerResponse,
  ) {
    const url = new URL(req.url ?? '/', 'http://localhost')
    const path = url.pathname.startsWith(prefix)
      ? url.pathname.slice(prefix.length) || '/'
      : null
    requests.push({
      path: path ?? url.pathname,
      query: url.search,
      authorization: req.headers.authorization,
    })
    if (state.down || path === null) return json(res, 503, { error: 'down' })

    // The signed address of a bundle: no login is needed and none must be sent.
    const file = /^\/files\/([^/]+)\/([^/]+)\.xt$/.exec(path)
    if (file) {
      const version = extensions
        .find((extension) => extension.slug === file[1])
        ?.versions.find((candidate) => candidate.version === file[2])
      if (!version) return json(res, 404, { error: 'unknown' })
      const bytes = bytesOf(version)
      res.writeHead(200, {
        'content-type': 'application/octet-stream',
        'content-length': bytes.length,
      })
      return void res.end(bytes)
    }

    if (token !== undefined && req.headers.authorization !== `Bearer ${token}`)
      return json(res, 401, { error: 'login required' })

    if (path === '/health') return json(res, 200, { status: 'ok' })

    if (path === '/categories') {
      const counts = new Map<string, number>()
      for (const extension of extensions)
        counts.set(
          extension.category,
          (counts.get(extension.category) ?? 0) + 1,
        )
      return json(res, 200, {
        categories: [...counts].map(([slug, count]) => ({
          name: CATEGORY_NAMES[slug] ?? slug,
          slug,
          extensionCount: count,
        })),
      })
    }

    if (path === '/extensions') {
      const search = url.searchParams.get('search')?.toLowerCase()
      const category = url.searchParams.get('category')
      const page = Math.max(1, Number(url.searchParams.get('page') ?? 1))
      const limit = Math.min(20, Number(url.searchParams.get('limit') ?? 20))
      const matching = extensions
        .filter(
          (extension) =>
            (!category || extension.category === category) &&
            (!search ||
              extension.name.toLowerCase().includes(search) ||
              extension.description.toLowerCase().includes(search)),
        )
        .sort((a, b) => b.downloads - a.downloads)
      return json(res, 200, {
        extensions: matching
          .slice((page - 1) * limit, page * limit)
          .map(entryJson),
        pagination: {
          page,
          limit,
          total: matching.length,
          totalPages: Math.ceil(matching.length / limit),
        },
      })
    }

    const download = /^\/extensions\/([^/]+)\/download$/.exec(path)
    if (download) {
      const extension = extensions.find(
        (candidate) => candidate.slug === download[1],
      )
      const wanted = url.searchParams.get('version')
      const version = extension?.versions.find(
        (candidate) => candidate.version === wanted,
      )
      if (!extension || !version) return json(res, 404, { error: 'unknown' })
      const listed = versionJson(version)
      return json(res, 200, {
        downloadUrl: `${origin}${prefix}/files/${extension.slug}/${version.version}.xt`,
        version: version.version,
        bundleSize: listed.bundleSize,
        ...('bundleHash' in listed ? { bundleHash: listed.bundleHash } : {}),
      })
    }

    const detail = /^\/extensions\/([^/]+)$/.exec(path)
    if (detail) {
      const extension = extensions.find(
        (candidate) => candidate.slug === detail[1],
      )
      if (!extension) return json(res, 404, { error: 'unknown' })
      return json(res, 200, {
        ...entryJson(extension),
        description: extension.description,
        publicKey: FIXTURE_KEY,
        author: 'haex',
      })
    }

    return json(res, 404, { error: 'unknown' })
  }

  const server: Server = createServer(handle)
  await new Promise<void>((resolve) => server.listen(0, '127.0.0.1', resolve))
  const { port } = server.address() as AddressInfo
  reachableFromDevice(port)
  origin = `http://127.0.0.1:${port}`

  return {
    url: `${origin}${prefix}`,
    origin,
    token,
    requests,
    extensions,
    get hash() {
      return state.hash
    },
    set hash(value: HashSwitch) {
      state.hash = value
    },
    get size() {
      return state.size
    },
    set size(value: SizeSwitch) {
      state.size = value
    },
    get down() {
      return state.down
    },
    set down(value: boolean) {
      state.down = value
    },
    hits: (path) => requests.filter((request) => request.path === path).length,
    close: () =>
      new Promise<void>((resolve) => {
        server.closeAllConnections()
        server.close(() => resolve())
      }),
  }
}

/** Makes a debug build point the preset marketplace of the vaults it creates next at `address`;
 * returns what puts the previous value back. */
export function pointPresetAt(address: string): () => void {
  const before = process.env[ADDRESS_VARIABLE]
  process.env[ADDRESS_VARIABLE] = address
  return () => {
    if (before === undefined)
      Reflect.deleteProperty(process.env, ADDRESS_VARIABLE)
    else process.env[ADDRESS_VARIABLE] = before
  }
}

/** Opens the catalog from the launcher, fits its window into the virtual screen and waits for it. */
export async function openMarketplace(instance: FlowInstance): Promise<void> {
  await instance.click('open-launcher')
  await instance.click('[data-app-id="system.marketplace"]')
  await instance.waitForDisplayed('marketplace-app')
  await resizeAppWindow(instance, 'system.marketplace', 900, 600)
  await instance.waitForDisplayed('marketplace-search')
}

/** The text content of the element with this test id (empty when there is none). */
export async function textOf(
  instance: FlowInstance,
  hook: string,
): Promise<string> {
  return instance.exec<string>(
    `return document.querySelector('[data-testid="' + arguments[0] + '"]')?.textContent?.trim() ?? ''`,
    [hook],
  )
}

/** How many elements have a test id that starts with `prefix`, among the tiles (`li`). */
export async function tileCount(instance: FlowInstance): Promise<number> {
  return instance.exec<number>(
    `return document.querySelectorAll('li[data-testid^="marketplace-tile-"]').length`,
  )
}

/** The slugs of the shown tiles, in order. */
export async function tileSlugs(instance: FlowInstance): Promise<string[]> {
  return instance.exec<string[]>(
    `return [...document.querySelectorAll('li[data-testid^="marketplace-tile-"]')].map((li) => li.getAttribute('data-testid').replace('marketplace-tile-', ''))`,
  )
}

/** Whether an element with this test id is in the page and not disabled. */
export async function enabled(
  instance: FlowInstance,
  hook: string,
): Promise<boolean> {
  return instance.exec<boolean>(
    `const el = document.querySelector('[data-testid="' + arguments[0] + '"]'); return !!el && !el.disabled`,
    [hook],
  )
}

/** The preset's id, which the scenarios address tiles by. */
export const PRESET_ID = 'preset-public'

/** Installs `slug` from its tile as a user does: the button, the preview, the confirmation, and
 * waits until the tile says it is installed. */
export async function installFromCatalog(
  instance: FlowInstance,
  slug: string,
): Promise<void> {
  await instance.click(`marketplace-install-${PRESET_ID}-${slug}`)
  await instance.waitForDisplayed('extensions-install-dialog', 30_000)
  await instance.click('extensions-install-confirm')
  await instance.waitForDisplayed(
    `marketplace-installed-${PRESET_ID}-${slug}`,
    30_000,
  )
}

/** The names and versions of the installed extensions. */
export async function installedVersions(
  instance: FlowInstance,
): Promise<Record<string, string>> {
  const list = unwrap<Array<{ name: string; version?: string; state: string }>>(
    'extension_list',
    await instance.invoke('extension_list'),
  )
  return Object.fromEntries(
    list
      .filter((extension) => extension.state === 'installed')
      .map((extension) => [extension.name, extension.version ?? '']),
  )
}

interface ListedMarketplace {
  id: string
  name: string
  baseUrl: string
  enabled: boolean
  rank: number
  authType: string
  /** `none`, `present`, `missing` or `syncing`. */
  credentials: string
  credentialsItemId?: string
  credentialsEntry?: { id: string; title: string | null; owned: boolean }
}

/** The marketplaces of the vault as `marketplace_list` reports them. */
export async function marketplaces(
  instance: Pick<FlowInstance, 'invoke'>,
): Promise<ListedMarketplace[]> {
  return unwrap<ListedMarketplace[]>(
    'marketplace_list',
    await instance.invoke('marketplace_list'),
  )
}

/**
 * Points the preset row at `url` through the same command the settings form uses. The variable
 * of `pointPresetAt` reaches only a debug build started from this machine; a phone's app never
 * sees it, so the scenarios repoint the row the way a user would, which works on every platform.
 * A row that already holds `url` (or is not there) is left alone.
 */
export async function repointPreset(
  instance: FlowInstance,
  url: string,
): Promise<void> {
  const preset = (await marketplaces(instance)).find(
    (market) => market.id === PRESET_ID,
  )
  if (!preset || preset.baseUrl === url) return
  unwrap(
    'marketplace_save',
    await instance.invoke('marketplace_save', {
      input: {
        id: PRESET_ID,
        name: preset.name,
        baseUrl: url,
        authType: 'none',
      },
    }),
  )
}
