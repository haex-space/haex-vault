#!/usr/bin/env node
/**
 * Android Init Script
 *
 * Initializes the Android build environment for Tauri.
 * Applies necessary patches after generation.
 */

import { execSync } from 'child_process'
import { readFileSync, writeFileSync, rmSync, existsSync } from 'fs'
import { join } from 'path'

const ANDROID_DIR = 'src-tauri/gen/android'

console.log('🤖 Initializing Android build environment...\n')

// Step 1: Remove existing Android directory
if (existsSync(ANDROID_DIR)) {
  console.log('🗑️  Removing existing Android directory...')
  rmSync(ANDROID_DIR, { recursive: true })
}

// Step 2: Run tauri android init
console.log('📱 Running tauri android init...')
execSync('pnpm tauri android init', { stdio: 'inherit' })

// Step 3: Apply patches
console.log('\n🔧 Applying patches...\n')

// Patch 1: Update Kotlin version
const buildGradlePath = join(ANDROID_DIR, 'build.gradle.kts')
let buildGradle = readFileSync(buildGradlePath, 'utf-8')
buildGradle = buildGradle.replace(
  'kotlin-gradle-plugin:1.9.25',
  'kotlin-gradle-plugin:2.1.0'
)
writeFileSync(buildGradlePath, buildGradle)
console.log('  ✓ Updated Kotlin version to 2.1.0')

// Patch 2: Fix rootDirRel path (Tauri bug - path is relative to app/, not android/)
const appBuildGradlePath = join(ANDROID_DIR, 'app/build.gradle.kts')
let appBuildGradle = readFileSync(appBuildGradlePath, 'utf-8')
appBuildGradle = appBuildGradle.replace(
  'rootDirRel = "../../../"',
  'rootDirRel = "../../../../"'
)
writeFileSync(appBuildGradlePath, appBuildGradle)
console.log('  ✓ Fixed rootDirRel path for local development')

console.log('\n✅ Android init complete!\n')
