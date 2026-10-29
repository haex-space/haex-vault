/**
 * The prebuilt sherpa-onnx libraries holzi links (spec 066, T001, research R3).
 *
 * `sherpa-onnx-sys`'s build script downloads them from GitHub without checking a checksum. holzi
 * hands them over through `SHERPA_ONNX_ARCHIVE_DIR` (root `.cargo/config.toml`) instead, after
 * `scripts/fetch-sherpa-onnx.ts` has checked each one against the SHA-256 below. The digests are
 * the ones GitHub lists for the release assets.
 */

/** Must equal the exact pin of `sherpa-onnx` in src-tauri/Cargo.toml. */
export const SHERPA_ONNX_VERSION = '2.0.1'

/** Relative to the repository root; `.cargo/config.toml` points cargo here. */
export const ARCHIVE_DIR = 'src-tauri/target/sherpa-onnx-archives'

export interface SherpaArchive {
  name: string
  sha256: string
  size: number
}

const v = SHERPA_ONNX_VERSION

/**
 * Desktop links statically (onnxruntime inside, nothing to ship next to the binary); Android and
 * iOS only exist as shared builds, which `sherpa-onnx-sys` picks on its own.
 */
const BY_TARGET: Record<string, SherpaArchive> = {
  'x86_64-unknown-linux-gnu': {
    name: `sherpa-onnx-v${v}-linux-x64-static-lib.tar.bz2`,
    sha256: '55cfadf65ba50d0afc1a802b7a337942819e51c94a5b5a384def1978f7c70871',
    size: 22_694_241,
  },
  'aarch64-unknown-linux-gnu': {
    name: `sherpa-onnx-v${v}-linux-aarch64-static-lib.tar.bz2`,
    sha256: 'b6c6090e40fd18d6812f04fe21260602cd1223d7b5e83ad39dab87ad7c1c031d',
    size: 21_076_492,
  },
  'x86_64-apple-darwin': {
    name: `sherpa-onnx-v${v}-osx-x64-static-lib.tar.bz2`,
    sha256: '6a3cb3c14104d3a02e7b2a6cd5621f590b14892cce7d0bc0935f7655ef518938',
    size: 20_415_061,
  },
  'aarch64-apple-darwin': {
    name: `sherpa-onnx-v${v}-osx-arm64-static-lib.tar.bz2`,
    sha256: 'd7a9b14f337958e7373c8c650f046df33d210bbaff2f33cb8322dda663ee5e1c',
    size: 20_829_454,
  },
  'x86_64-pc-windows-msvc': {
    name: `sherpa-onnx-v${v}-win-x64-static-MT-Release-lib.tar.bz2`,
    sha256: '23e7b986b197eda0bbc47ff3d7d5c87afd6ef2b9077214ba26b0f1ec52ef0e66',
    size: 122_863_596,
  },
  'aarch64-pc-windows-msvc': {
    name: `sherpa-onnx-v${v}-win-arm64-static-MT-Release-lib.tar.bz2`,
    sha256: 'ef118817dcf0a585974159855ff8b2a0f4c8069a158766dfd92a5f702b0173be',
    size: 140_877_151,
  },
  ...Object.fromEntries(
    [
      'aarch64-linux-android',
      'armv7-linux-androideabi',
      'i686-linux-android',
      'x86_64-linux-android',
    ].map((target) => [
      target,
      {
        name: `sherpa-onnx-v${v}-android.tar.bz2`,
        sha256:
          '39003753ec293425ed56723ea9b972b98841a490af3311670c3060548d654330',
        size: 45_300_379,
      },
    ]),
  ),
  ...Object.fromEntries(
    ['aarch64-apple-ios', 'aarch64-apple-ios-sim', 'x86_64-apple-ios'].map(
      (target) => [
        target,
        {
          name: `sherpa-onnx-v${v}-ios-shared-onnxruntime-static.xcframework.zip`,
          sha256:
            'fc4c665e694322ced5393aafafaae6bb3e0a7449d98d5db60bc220ac7ac4cbc9',
          size: 31_577_060,
        },
      ],
    ),
  ),
}

/** The archive `sherpa-onnx-sys` looks for when building for `target`. */
export function archiveFor(target: string): SherpaArchive {
  const archive = BY_TARGET[target]
  if (!archive) {
    throw new Error(`no sherpa-onnx archive is pinned for target ${target}`)
  }
  return archive
}

export function knownTargets(): string[] {
  return Object.keys(BY_TARGET)
}

/** Same layout as `download_url` in `sherpa-onnx-sys`'s build script. */
export function downloadUrl(archive: SherpaArchive): string {
  const base = 'https://github.com/k2-fsa/sherpa-onnx/releases/download'
  return archive.name.endsWith('.xcframework.zip')
    ? `${base}/xcframework/${archive.name}`
    : `${base}/v${SHERPA_ONNX_VERSION}/${archive.name}`
}

/** The Rust target of the machine running node, for the desktops holzi builds on. */
export function hostTarget(
  platform: NodeJS.Platform = process.platform,
  arch: string = process.arch,
): string {
  const cpu = arch === 'arm64' ? 'aarch64' : arch === 'x64' ? 'x86_64' : arch
  if (platform === 'linux') return `${cpu}-unknown-linux-gnu`
  if (platform === 'darwin') return `${cpu}-apple-darwin`
  if (platform === 'win32') return `${cpu}-pc-windows-msvc`
  throw new Error(`unsupported host ${platform}/${arch}`)
}
