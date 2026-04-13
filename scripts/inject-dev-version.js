#!/usr/bin/env node

// Injects a dev-version suffix into the version files (working tree only —
// does NOT commit). Used for non-release builds (CI on main, local dev,
// nightly builds) so the resulting binary's version is unambiguously
// distinguishable from a real release.
//
// Format: <base-version>-dev.<short-sha>
//   e.g. "1.9.0" + "5f3a2c1" → "1.9.0-dev.5f3a2c1"
//
// SemVer 2.0.0 conformant: the "-dev.<sha>" portion is a pre-release
// identifier, which sorts BEFORE the same base version without a suffix
// (so "1.9.0-dev.5f3a2c1" < "1.9.0"). That's intentional — a dev build of
// 1.9.0 should be considered earlier than the released 1.9.0.
//
// Files patched:
//   - package.json
//   - src-tauri/tauri.conf.json
//   - src-tauri/Cargo.toml
//
// Cargo.lock is updated by running `cargo check` afterwards (caller's
// responsibility — this script only touches the source files).

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

const shortSha = execFileSync('git', ['rev-parse', '--short', 'HEAD'], { encoding: 'utf8' }).trim();
const newVersion = `${baseVersion}-dev.${shortSha}`;

console.log(`Injecting dev version: ${baseVersion} → ${newVersion}`);

pkg.version = newVersion;
writeFileSync(packageJsonPath, JSON.stringify(pkg, null, 2) + '\n');

const tauriConf = JSON.parse(readFileSync(tauriConfPath, 'utf8'));
tauriConf.version = newVersion;
writeFileSync(tauriConfPath, JSON.stringify(tauriConf, null, 2) + '\n');

const cargoToml = readFileSync(cargoTomlPath, 'utf8');
const updatedCargo = cargoToml.replace(/^version = ".*"$/m, `version = "${newVersion}"`);
writeFileSync(cargoTomlPath, updatedCargo);

console.log('Patched: package.json, src-tauri/tauri.conf.json, src-tauri/Cargo.toml');
console.log('Note: run `cd src-tauri && cargo check` to update Cargo.lock');
