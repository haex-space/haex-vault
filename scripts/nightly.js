#!/usr/bin/env node

import { readFileSync } from 'fs';
import { execSync } from 'child_process';
import { fileURLToPath } from 'url';
import { dirname, join } from 'path';

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);
const rootDir = join(__dirname, '..');

// Get current date and short commit SHA
const buildDate = new Date().toISOString().slice(0, 10).replace(/-/g, '');
const shortSha = execSync('git rev-parse --short HEAD', { encoding: 'utf8' }).trim();

// Read current package.json for version info
const packageJsonPath = join(rootDir, 'package.json');
const packageJson = JSON.parse(readFileSync(packageJsonPath, 'utf8'));
const currentVersion = packageJson.version;

const nightlyTag = `nightly-${buildDate}-${shortSha}`;

console.log(`🌙 Creating nightly build: ${nightlyTag}`);
console.log(`📦 Base version: ${currentVersion}`);
console.log(`📅 Build date: ${buildDate}`);
console.log(`🔗 Commit: ${shortSha}`);

try {
  // Check if there are uncommitted changes
  const status = execSync('git status --porcelain', { encoding: 'utf8' });
  if (status.trim()) {
    console.error('❌ There are uncommitted changes. Please commit or stash them first.');
    process.exit(1);
  }

  // Check if tag already exists
  try {
    execSync(`git rev-parse ${nightlyTag}`, { encoding: 'utf8', stdio: 'pipe' });
    console.error(`❌ Tag ${nightlyTag} already exists. A nightly has already been created for this commit today.`);
    process.exit(1);
  } catch {
    // Tag doesn't exist, we can continue
  }

  // Create nightly tag
  execSync(`git tag ${nightlyTag}`, { stdio: 'inherit' });
  console.log(`✅ Created tag ${nightlyTag}`);

  // Push tag
  console.log('📤 Pushing tag to remote...');
  execSync(`git push origin ${nightlyTag}`, { stdio: 'inherit' });
  console.log('✅ Pushed nightly tag');

  console.log('\n🎉 Nightly tag created successfully!');
  console.log('📋 Trigger the nightly workflow manually on GitHub Actions to build.');
  console.log(`   Or wait for the scheduled run at 2:00 UTC.`);
} catch (error) {
  console.error('❌ Operation failed:', error.message);

  // Try to delete the local tag if it was created
  try {
    execSync(`git tag -d ${nightlyTag}`, { stdio: 'pipe' });
    console.log(`↩️  Deleted local tag ${nightlyTag}`);
  } catch {
    // Tag might not have been created
  }

  process.exit(1);
}
