// playwright.config.ts
//
// Playwright E2E Test Configuration for Haex Vault
//

import { defineConfig, devices } from '@playwright/test'

export default defineConfig({
  testDir: './e2e',
  fullyParallel: true,
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 2 : 0,
  workers: process.env.CI ? 1 : undefined,
  reporter: [['html'], ['list']],

  // Global timeout for each test
  timeout: 60000,

  // Global expect timeout
  expect: {
    timeout: 10000,
  },

  use: {
    // Base URL for the dev server
    baseURL: 'http://localhost:3000',
    trace: 'on-first-retry',
    screenshot: 'only-on-failure',
    video: 'retain-on-failure',

    // Viewport settings
    viewport: { width: 1280, height: 720 },

    // Action timeouts
    actionTimeout: 15000,
    navigationTimeout: 30000,
  },

  projects: [
    {
      name: 'chromium',
      use: { ...devices['Desktop Chrome'] },
    },
    {
      name: 'firefox',
      use: { ...devices['Desktop Firefox'] },
    },
  ],

  // Configure the dev server to start before running tests
  webServer: [
    {
      command: 'pnpm dev',
      url: 'http://localhost:3000',
      reuseExistingServer: !process.env.CI,
      timeout: 120000,
    },
    // Optionally start haex-sync-server if available
    // Uncomment and adjust path if you have the server in a sibling directory
    // {
    //   command: 'npm run dev',
    //   url: 'http://localhost:3002',
    //   cwd: '../haex-sync-server',
    //   reuseExistingServer: !process.env.CI,
    //   timeout: 60000,
    // },
  ],
})
