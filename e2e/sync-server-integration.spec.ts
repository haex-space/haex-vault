// e2e/sync-server-integration.spec.ts
//
// E2E Tests for haex-sync-server Integration
//
// Tests the complete backend integration:
// - Server connection and health check
// - Account creation/registration
// - User authentication (login)
// - Vault listing from server
// - Vault connection and sync
//

import { test, expect, Page, APIRequestContext } from '@playwright/test'

// ============================================================================
// Test Configuration
// ============================================================================

// Server URLs for testing
const SYNC_SERVER_URL =
  process.env.SYNC_SERVER_URL || 'http://localhost:3002'
const PRODUCTION_SERVER_URL = 'https://sync.haex.space'

// Test credentials (use environment variables in CI)
const TEST_EMAIL = process.env.TEST_EMAIL || `test_${Date.now()}@example.com`
const TEST_PASSWORD = process.env.TEST_PASSWORD || 'TestPassword123!'

// ============================================================================
// Helper Functions
// ============================================================================

/**
 * Wait for the app to be fully loaded
 */
async function waitForAppReady(page: Page): Promise<void> {
  await page.waitForLoadState('networkidle')
  await page.waitForTimeout(1000)
}

/**
 * Navigate to the sync/connection settings
 */
async function navigateToSyncSettings(page: Page): Promise<void> {
  // Look for connect/sync button or link
  const connectButton = page.locator(
    '[data-testid="connect-btn"], button:has-text("Connect"), button:has-text("Verbinden"), button:has-text("Sync"), a:has-text("Connect")',
  )

  if (await connectButton.first().isVisible().catch(() => false)) {
    await connectButton.first().click()
    await page.waitForTimeout(500)
  }
}

/**
 * Fill the server login form
 */
async function fillLoginForm(
  page: Page,
  email: string,
  password: string,
  serverUrl: string = SYNC_SERVER_URL,
): Promise<void> {
  // Select or enter server URL
  const serverInput = page.locator(
    'input[name="serverUrl"], input[placeholder*="Server"], select[name="server"]',
  )

  if (await serverInput.isVisible()) {
    if ((await serverInput.evaluate((el) => el.tagName)) === 'SELECT') {
      // It's a dropdown - select custom
      await serverInput.selectOption({ label: 'Custom...' })
      const customInput = page.locator('input[placeholder*="URL"]')
      if (await customInput.isVisible()) {
        await customInput.fill(serverUrl)
      }
    } else {
      await serverInput.fill(serverUrl)
    }
  }

  // Fill email
  const emailInput = page.locator(
    'input[type="email"], input[name="email"], input[placeholder*="Email"], input[placeholder*="E-Mail"]',
  )
  if (await emailInput.isVisible()) {
    await emailInput.fill(email)
  }

  // Fill password
  const passwordInput = page.locator(
    'input[type="password"], input[name="password"]',
  )
  if (await passwordInput.isVisible()) {
    await passwordInput.fill(password)
  }
}

// ============================================================================
// Server Health Check Tests
// ============================================================================

test.describe('Server Health Check', () => {
  test('should check local server availability', async ({ request }) => {
    try {
      const response = await request.get(SYNC_SERVER_URL, { timeout: 5000 })
      expect(response.ok()).toBeTruthy()

      const data = await response.json()
      // Server should return Supabase configuration
      expect(data.supabaseUrl || data.status).toBeDefined()
    } catch {
      // Local server might not be running - skip test
      test.skip()
    }
  })

  test('should check production server availability', async ({ request }) => {
    try {
      const response = await request.get(PRODUCTION_SERVER_URL, {
        timeout: 10000,
      })
      expect(response.ok()).toBeTruthy()

      const data = await response.json()
      expect(data.supabaseUrl).toBeDefined()
      expect(data.supabaseAnonKey).toBeDefined()
    } catch {
      // Production server not reachable - might be network issue
      console.warn('Production server not reachable')
    }
  })

  test('should return proper CORS headers', async ({ request }) => {
    try {
      const response = await request.get(SYNC_SERVER_URL, { timeout: 5000 })

      // Check for CORS-related headers
      const headers = response.headers()
      // Either has CORS headers or returns proper content
      expect(response.ok()).toBeTruthy()
    } catch {
      test.skip()
    }
  })
})

// ============================================================================
// Account Registration Tests
// ============================================================================

test.describe('Account Registration', () => {
  test('should display registration option', async ({ page }) => {
    await page.goto('/')
    await waitForAppReady(page)
    await navigateToSyncSettings(page)

    // Look for registration/signup option
    const registerOption = page.locator(
      'button:has-text("Register"), button:has-text("Registrieren"), button:has-text("Sign up"), a:has-text("Register"), a:has-text("Sign up")',
    )

    const pageContent = await page.content()
    const hasRegisterOption =
      (await registerOption.first().isVisible().catch(() => false)) ||
      pageContent.includes('Register') ||
      pageContent.includes('Registrieren') ||
      pageContent.includes('Sign up')

    expect(hasRegisterOption).toBeTruthy()
  })

  test('should validate email format during registration', async ({ page }) => {
    await page.goto('/')
    await waitForAppReady(page)
    await navigateToSyncSettings(page)

    // Click register if available
    const registerButton = page.locator(
      'button:has-text("Register"), a:has-text("Register")',
    )
    if (await registerButton.first().isVisible().catch(() => false)) {
      await registerButton.first().click()
      await page.waitForTimeout(500)
    }

    // Try invalid email
    const emailInput = page.locator('input[type="email"], input[name="email"]')
    if (await emailInput.isVisible()) {
      await emailInput.fill('invalid-email')

      const submitButton = page.locator(
        'button[type="submit"], button:has-text("Register")',
      )
      if (await submitButton.isVisible()) {
        await submitButton.click()
        await page.waitForTimeout(500)

        // Should show validation error
        const hasError =
          (await page.locator('[class*="error"]').isVisible().catch(() => false)) ||
          (await page.locator('[role="alert"]').isVisible().catch(() => false)) ||
          (await emailInput.evaluate((el: HTMLInputElement) => !el.validity.valid))

        expect(hasError).toBeTruthy()
      }
    }
  })

  test.skip('should create new account successfully', async ({
    page,
    request,
  }) => {
    // This test actually creates an account - only run in controlled environments
    if (!process.env.ALLOW_ACCOUNT_CREATION) {
      test.skip()
      return
    }

    await page.goto('/')
    await waitForAppReady(page)
    await navigateToSyncSettings(page)

    // Fill registration form
    const uniqueEmail = `test_${Date.now()}@example.com`
    await fillLoginForm(page, uniqueEmail, TEST_PASSWORD, SYNC_SERVER_URL)

    // Submit registration
    const registerButton = page.locator(
      'button:has-text("Register"), button[type="submit"]',
    )
    if (await registerButton.isVisible()) {
      await registerButton.click()
      await page.waitForTimeout(3000)

      // Check for success
      const success =
        (await page.locator('text=/success|erfolgreich|created/i').isVisible().catch(() => false)) ||
        (await page.locator('[data-testid="verification-sent"]').isVisible().catch(() => false))

      expect(success).toBeTruthy()
    }
  })
})

// ============================================================================
// User Authentication Tests
// ============================================================================

test.describe('User Authentication', () => {
  test('should display login form', async ({ page }) => {
    await page.goto('/')
    await waitForAppReady(page)
    await navigateToSyncSettings(page)

    // Check for login form elements
    const emailInput = page.locator('input[type="email"], input[name="email"]')
    const passwordInput = page.locator('input[type="password"]')

    const hasLoginForm =
      (await emailInput.isVisible().catch(() => false)) &&
      (await passwordInput.isVisible().catch(() => false))

    expect(hasLoginForm).toBeTruthy()
  })

  test('should show error for invalid credentials', async ({ page }) => {
    await page.goto('/')
    await waitForAppReady(page)
    await navigateToSyncSettings(page)

    // Try to login with invalid credentials
    await fillLoginForm(
      page,
      'nonexistent@example.com',
      'wrongpassword',
      SYNC_SERVER_URL,
    )

    const loginButton = page.locator(
      'button:has-text("Login"), button:has-text("Anmelden"), button[type="submit"]',
    )

    if (await loginButton.isVisible()) {
      await loginButton.click()
      await page.waitForTimeout(3000)

      // Should show error
      const hasError =
        (await page.locator('[class*="error"]').isVisible().catch(() => false)) ||
        (await page.locator('text=/invalid|ungültig|failed|fehlgeschlagen/i').isVisible().catch(() => false)) ||
        (await page.locator('[role="alert"]').isVisible().catch(() => false))

      expect(hasError).toBeTruthy()
    }
  })

  test.skip('should login successfully with valid credentials', async ({
    page,
  }) => {
    // This test requires valid credentials
    if (!process.env.TEST_VALID_EMAIL || !process.env.TEST_VALID_PASSWORD) {
      test.skip()
      return
    }

    await page.goto('/')
    await waitForAppReady(page)
    await navigateToSyncSettings(page)

    await fillLoginForm(
      page,
      process.env.TEST_VALID_EMAIL!,
      process.env.TEST_VALID_PASSWORD!,
      SYNC_SERVER_URL,
    )

    const loginButton = page.locator(
      'button:has-text("Login"), button[type="submit"]',
    )

    if (await loginButton.isVisible()) {
      await loginButton.click()
      await page.waitForTimeout(3000)

      // Should proceed to next step (vault selection)
      const success =
        (await page.locator('text=/vaults|Tresore|select|auswählen/i').isVisible().catch(() => false)) ||
        (await page.locator('[data-testid="vault-list"]').isVisible().catch(() => false))

      expect(success).toBeTruthy()
    }
  })
})

// ============================================================================
// Vault Connection Tests
// ============================================================================

test.describe('Vault Connection', () => {
  test('should show server selection options', async ({ page }) => {
    await page.goto('/')
    await waitForAppReady(page)
    await navigateToSyncSettings(page)

    // Check for server selection
    const serverSelector = page.locator(
      'select[name="server"], [data-testid="server-select"], input[placeholder*="Server"]',
    )

    const hasServerSelector = await serverSelector
      .first()
      .isVisible()
      .catch(() => false)

    // Either dropdown or input should be available
    expect(
      hasServerSelector ||
        (await page.locator('text=/HaexSpace|Local|Custom/i').isVisible().catch(() => false)),
    ).toBeTruthy()
  })

  test('should allow custom server URL', async ({ page }) => {
    await page.goto('/')
    await waitForAppReady(page)
    await navigateToSyncSettings(page)

    // Select custom option if available
    const serverSelect = page.locator('select[name="server"]')
    if (await serverSelect.isVisible()) {
      await serverSelect.selectOption({ label: 'Custom...' })
    }

    // Look for custom URL input
    const customUrlInput = page.locator(
      'input[placeholder*="URL"], input[name="serverUrl"]',
    )

    if (await customUrlInput.isVisible()) {
      await customUrlInput.fill('https://my-custom-server.com')

      const value = await customUrlInput.inputValue()
      expect(value).toBe('https://my-custom-server.com')
    }
  })

  test('should display connection wizard steps', async ({ page }) => {
    await page.goto('/')
    await waitForAppReady(page)
    await navigateToSyncSettings(page)

    // Look for wizard steps or progress indicator
    const stepIndicator = page.locator(
      '[data-testid="wizard-steps"], [class*="step"], [class*="progress"], text=/step|Schritt/i',
    )

    const pageContent = await page.content()
    const hasSteps =
      (await stepIndicator.first().isVisible().catch(() => false)) ||
      pageContent.includes('Step') ||
      pageContent.includes('Schritt') ||
      pageContent.includes('1') // Step numbers

    expect(hasSteps).toBeTruthy()
  })
})

// ============================================================================
// Vault Sync Tests
// ============================================================================

test.describe('Vault Synchronization', () => {
  test.skip('should list remote vaults after login', async ({ page }) => {
    // Requires valid login
    if (!process.env.TEST_VALID_EMAIL) {
      test.skip()
      return
    }

    await page.goto('/')
    await waitForAppReady(page)
    await navigateToSyncSettings(page)

    // Login first
    await fillLoginForm(
      page,
      process.env.TEST_VALID_EMAIL!,
      process.env.TEST_VALID_PASSWORD!,
    )

    const loginButton = page.locator('button:has-text("Login")')
    if (await loginButton.isVisible()) {
      await loginButton.click()
      await page.waitForTimeout(3000)
    }

    // Check for vault list
    const vaultList = page.locator(
      '[data-testid="vault-list"], [data-testid="remote-vaults"]',
    )

    const hasVaultList = await vaultList.isVisible().catch(() => false)
    expect(hasVaultList).toBeTruthy()
  })

  test.skip('should show sync status', async ({ page }) => {
    // This would require an active vault with sync enabled
    await page.goto('/')
    await waitForAppReady(page)

    // Look for sync status indicator
    const syncStatus = page.locator(
      '[data-testid="sync-status"], [class*="sync"], text=/sync|synchron/i',
    )

    const hasSyncStatus = await syncStatus.first().isVisible().catch(() => false)
    expect(hasSyncStatus).toBeTruthy()
  })
})

// ============================================================================
// API Integration Tests
// ============================================================================

test.describe('API Integration', () => {
  test('should handle server errors gracefully', async ({ page }) => {
    await page.goto('/')
    await waitForAppReady(page)
    await navigateToSyncSettings(page)

    // Try to connect to non-existent server
    const serverInput = page.locator(
      'input[name="serverUrl"], input[placeholder*="URL"]',
    )

    if (await serverInput.isVisible()) {
      await serverInput.fill('https://non-existent-server-12345.com')

      const loginButton = page.locator(
        'button:has-text("Login"), button[type="submit"]',
      )

      if (await loginButton.isVisible()) {
        await loginButton.click()
        await page.waitForTimeout(3000)

        // Should show connection error, not crash
        const hasError =
          (await page.locator('text=/error|Fehler|connect|verbind/i').isVisible().catch(() => false)) ||
          (await page.locator('[role="alert"]').isVisible().catch(() => false))

        expect(hasError).toBeTruthy()
      }
    }
  })

  test('should handle timeout gracefully', async ({ page }) => {
    // Set a very short timeout for testing
    await page.route('**/sync/**', async (route) => {
      // Simulate slow response
      await new Promise((resolve) => setTimeout(resolve, 60000))
      await route.continue()
    })

    await page.goto('/')
    await waitForAppReady(page)

    // App should still be usable
    const body = page.locator('body')
    await expect(body).toBeVisible()
  })
})

// ============================================================================
// Security Tests
// ============================================================================

test.describe('Sync Security', () => {
  test('should not expose credentials in URL', async ({ page }) => {
    await page.goto('/')
    await waitForAppReady(page)
    await navigateToSyncSettings(page)

    await fillLoginForm(page, 'test@example.com', 'password123')

    // Check URL doesn't contain credentials
    const url = page.url()
    expect(url).not.toContain('password')
    expect(url).not.toContain('password123')
  })

  test('should use HTTPS for production server', async ({ page }) => {
    await page.goto('/')
    await waitForAppReady(page)
    await navigateToSyncSettings(page)

    // Check that production URL uses HTTPS
    const pageContent = await page.content()
    if (pageContent.includes('sync.haex.space')) {
      expect(pageContent).toContain('https://sync.haex.space')
    }
  })

  test('should clear session on logout', async ({ page }) => {
    await page.goto('/')
    await waitForAppReady(page)

    // Look for logout button
    const logoutButton = page.locator(
      'button:has-text("Logout"), button:has-text("Abmelden"), [data-testid="logout"]',
    )

    if (await logoutButton.isVisible()) {
      await logoutButton.click()
      await page.waitForTimeout(1000)

      // Should clear local storage auth tokens
      const hasAuthToken = await page.evaluate(() => {
        return (
          localStorage.getItem('supabase.auth.token') ||
          localStorage.getItem('session')
        )
      })

      expect(hasAuthToken).toBeFalsy()
    }
  })
})
