#!/usr/bin/env node

// Injects a numeric pre-release suffix into the version files (working tree
// only — does NOT commit). Used for non-release builds (CI on main, local dev,
// nightly builds) so the resulting binary's version is unambiguously
// distinguishable from a real release.
//
// Format: <base-version>-<commits-since-tag>
//   e.g. base "1.9.0" + 5 commits after v1.9.0 tag → "1.9.0-5"
//
// Why numeric-only:
//   The Windows MSI bundler enforces "optional pre-release identifier must
//   be numeric-only and cannot be greater than 65535". Alphanumeric suffixes
//   like "-dev.5f3a2c1" fail MSI bundling. Numeric commit count works on
//   all platforms (Windows MSI, macOS DMG, Linux DEB/AppImage).
//
// SemVer 2.0.0 conformant: "1.9.0-5" is a pre-release of "1.9.0" and sorts
// BEFORE the released "1.9.0" — exactly what we want for dev builds.
//
// Files patched:
//   - package.json
//   - src-tauri/tauri.conf.json
//   - src-tauri/Cargo.toml
//
// Cargo.lock: NOT updated by this script. The downstream `cargo build` /
// `tauri build` will automatically refresh the lock entry for the local
// crate when it sees the version mismatch (cargo's default behavior — only
// `--locked` / `--frozen` would fail, and tauri-action doesn't pass those).
// Avoiding `cargo check` here is important: on Android-build runners that
// step would compile the desktop dep tree (gdk-sys etc) and fail.

import { readFileSync, writeFileSync } from 'fs';
import { execFileSync } from 'child_process';
import { fileURLToPath } from 'url';
import { dirname, join } from 'path';

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);
const rootDir = join(__dirname, '..');

const packageJsonPath = join(rootDir, 'package.json');
const tauriConfPath = join(rootDir, 'src-tauri/tauri.conf.json');
const cargoTomlPath = join(rootDir, 'src-tauri/Cargo.toml');

const pkg = JSON.parse(readFileSync(packageJsonPath, 'utf8'));
const baseVersion = pkg.version;

if (/-/.test(baseVersion)) {
  console.log(`Version "${baseVersion}" already has a pre-release suffix — skipping injection.`);
  process.exit(0);
}

// Count commits since the corresponding release tag. If no such tag exists
// or HEAD is exactly at the tag, skip injection (the build is at a release).
let commitCount;
try {
  const out = execFileSync(
    'git',
    ['rev-list', '--count', `v${baseVersion}..HEAD`],
    { encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'] }
  );
  commitCount = parseInt(out.trim(), 10);
} catch {
  console.log(`Tag v${baseVersion} not found — skipping injection (cannot derive commit count).`);
  process.exit(0);
}

if (!Number.isFinite(commitCount) || commitCount <= 0) {
  console.log(`HEAD is at v${baseVersion} (no commits ahead) — skipping injection.`);
  process.exit(0);
}

if (commitCount > 65535) {
  console.warn(`::warning::Commit count ${commitCount} exceeds MSI limit (65535) — clamping.`);
  commitCount = 65535;
}

const newVersion = `${baseVersion}-${commitCount}`;
console.log(`Injecting dev version: ${baseVersion} → ${newVersion} (${commitCount} commits since v${baseVersion})`);

pkg.version = newVersion;
writeFileSync(packageJsonPath, JSON.stringify(pkg, null, 2) + '\n');

const tauriConf = JSON.parse(readFileSync(tauriConfPath, 'utf8'));
tauriConf.version = newVersion;
writeFileSync(tauriConfPath, JSON.stringify(tauriConf, null, 2) + '\n');

const cargoToml = readFileSync(cargoTomlPath, 'utf8');
const updatedCargo = cargoToml.replace(/^version = ".*"$/m, `version = "${newVersion}"`);
writeFileSync(cargoTomlPath, updatedCargo);

console.log('Patched: package.json, src-tauri/tauri.conf.json, src-tauri/Cargo.toml');
