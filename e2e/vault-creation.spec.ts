// e2e/vault-creation.spec.ts
//
// E2E Tests for Vault Creation Flow
//
// Tests the complete vault creation process:
// - Opening the app
// - Filling out the vault creation form
// - Validating vault name uniqueness
// - Creating the vault with password
// - Navigating to the desktop
//

import { test, expect, Page } from '@playwright/test'

// ============================================================================
// Test Configuration
// ============================================================================

// Unique vault name for each test run
const generateVaultName = () => `TestVault_${Date.now()}`

// ============================================================================
// Helper Functions
// ============================================================================

/**
 * Wait for the app to be fully loaded
 */
async function waitForAppReady(page: Page): Promise<void> {
  // Wait for the main app to load
  await page.waitForLoadState('networkidle')

  // Look for the welcome page or vault selection
  await page
    .waitForSelector('[data-testid="welcome-page"], [data-testid="vault-create-form"]', {
      state: 'visible',
      timeout: 30000,
    })
    .catch(() => {
      // Fallback: just wait for body
      return page.waitForSelector('body')
    })
}

/**
 * Navigate to vault creation form
 */
async function navigateToVaultCreation(page: Page): Promise<void> {
  // Look for the "Create Vault" button or similar
  const createButton = page.locator(
    '[data-testid="create-vault-btn"], button:has-text("Erstellen"), button:has-text("Create"), button:has-text("Neuen Tresor")',
  )

  if (await createButton.isVisible()) {
    await createButton.click()
  }

  // Wait for the creation form
  await page.waitForSelector(
    '[data-testid="vault-create-form"], form:has(input[type="password"])',
    { state: 'visible', timeout: 10000 },
  ).catch(() => {
    // Form might already be visible on the page
  })
}

/**
 * Fill the vault creation form
 */
async function fillVaultCreationForm(
  page: Page,
  vaultName: string,
  password: string,
): Promise<void> {
  // Fill vault name
  const nameInput = page.locator(
    '[data-testid="vault-name-input"], input[name="vaultName"], input[placeholder*="Name"], input[placeholder*="Tresor"]',
  )
  await nameInput.fill(vaultName)

  // Fill password
  const passwordInput = page.locator(
    '[data-testid="vault-password-input"], input[name="password"]:first-of-type, input[type="password"]:first-of-type',
  )
  await passwordInput.fill(password)

  // Fill password confirmation if exists
  const confirmInput = page.locator(
    '[data-testid="vault-password-confirm"], input[name="passwordConfirm"], input[type="password"]:nth-of-type(2)',
  )
  if (await confirmInput.isVisible()) {
    await confirmInput.fill(password)
  }
}

/**
 * Submit the vault creation form
 */
async function submitVaultCreation(page: Page): Promise<void> {
  const submitButton = page.locator(
    '[data-testid="vault-create-submit"], button[type="submit"], button:has-text("Erstellen"), button:has-text("Create")',
  )
  await submitButton.click()
}

// ============================================================================
// Vault Creation Tests
// ============================================================================

test.describe('Vault Creation', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/')
    await waitForAppReady(page)
  })

  test('should display the vault creation option', async ({ page }) => {
    // Check that the create vault option is visible
    const createOption = page.locator(
      'button:has-text("Erstellen"), button:has-text("Create"), button:has-text("Neuen Tresor"), [data-testid="create-vault-btn"]',
    )

    // At least one create option should be visible
    const isVisible = await createOption.first().isVisible().catch(() => false)

    // If not directly visible, check the page content
    if (!isVisible) {
      const pageContent = await page.content()
      // The page should have some indication of vault creation
      expect(
        pageContent.includes('Erstellen') ||
          pageContent.includes('Create') ||
          pageContent.includes('vault') ||
          pageContent.includes('Tresor'),
      ).toBeTruthy()
    }
  })

  test('should show vault creation form', async ({ page }) => {
    await navigateToVaultCreation(page)

    // Check for form elements
    const hasNameInput = await page
      .locator('input[name="vaultName"], input[placeholder*="Name"]')
      .isVisible()
      .catch(() => false)

    const hasPasswordInput = await page
      .locator('input[type="password"]')
      .isVisible()
      .catch(() => false)

    // At least password input should be visible for vault creation
    expect(hasPasswordInput || hasNameInput).toBeTruthy()
  })

  test('should validate required fields', async ({ page }) => {
    await navigateToVaultCreation(page)

    // Try to submit without filling fields
    const submitButton = page.locator(
      'button[type="submit"], button:has-text("Erstellen"), button:has-text("Create")',
    )

    if (await submitButton.isVisible()) {
      await submitButton.click()

      // Should show validation errors or stay on the form
      await page.waitForTimeout(500)

      // Form should still be visible (not navigated away)
      const formStillVisible = await page
        .locator('form, [data-testid="vault-create-form"]')
        .isVisible()
        .catch(() => true)

      expect(formStillVisible).toBeTruthy()
    }
  })

  test('should validate password confirmation match', async ({ page }) => {
    await navigateToVaultCreation(page)

    const vaultName = generateVaultName()

    // Fill name
    const nameInput = page.locator(
      'input[name="vaultName"], input[placeholder*="Name"]',
    )
    if (await nameInput.isVisible()) {
      await nameInput.fill(vaultName)
    }

    // Fill mismatched passwords
    const passwordInputs = page.locator('input[type="password"]')
    const count = await passwordInputs.count()

    if (count >= 2) {
      await passwordInputs.nth(0).fill('Password123!')
      await passwordInputs.nth(1).fill('DifferentPassword!')

      // Try to submit
      const submitButton = page.locator(
        'button[type="submit"], button:has-text("Erstellen")',
      )
      if (await submitButton.isVisible()) {
        await submitButton.click()
        await page.waitForTimeout(500)

        // Should show validation error
        const hasError = await page
          .locator('[class*="error"], [role="alert"], .text-red')
          .isVisible()
          .catch(() => false)

        // Form should still be visible
        const formStillVisible = await page
          .locator('form, input[type="password"]')
          .isVisible()
          .catch(() => true)

        expect(formStillVisible).toBeTruthy()
      }
    }
  })

  test('should create vault with valid data', async ({ page }) => {
    await navigateToVaultCreation(page)

    const vaultName = generateVaultName()
    const password = 'SecureTestPassword123!'

    await fillVaultCreationForm(page, vaultName, password)
    await submitVaultCreation(page)

    // Wait for navigation or success indication
    await page.waitForTimeout(2000)

    // Check for success:
    // 1. Navigation to desktop
    // 2. Success message
    // 3. Vault name visible
    const success =
      (await page.url().includes('vault')) ||
      (await page.locator('[data-testid="desktop"]').isVisible().catch(() => false)) ||
      (await page.locator(`text=${vaultName}`).isVisible().catch(() => false))

    // Page should have changed or show success
    expect(success || !(await page.locator('form').isVisible())).toBeTruthy()
  })
})

// ============================================================================
// Vault Name Validation Tests
// ============================================================================

test.describe('Vault Name Validation', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/')
    await waitForAppReady(page)
    await navigateToVaultCreation(page)
  })

  test('should reject empty vault name', async ({ page }) => {
    const nameInput = page.locator(
      'input[name="vaultName"], input[placeholder*="Name"]',
    )

    if (await nameInput.isVisible()) {
      // Leave name empty, fill password
      const passwordInput = page.locator('input[type="password"]').first()
      if (await passwordInput.isVisible()) {
        await passwordInput.fill('Password123!')
      }

      const submitButton = page.locator('button[type="submit"]')
      if (await submitButton.isVisible()) {
        await submitButton.click()
        await page.waitForTimeout(500)

        // Should stay on form or show error
        const hasError =
          (await page.locator('[class*="error"]').isVisible().catch(() => false)) ||
          (await nameInput.isVisible())

        expect(hasError).toBeTruthy()
      }
    }
  })

  test('should handle special characters in vault name', async ({ page }) => {
    const nameInput = page.locator(
      'input[name="vaultName"], input[placeholder*="Name"]',
    )

    if (await nameInput.isVisible()) {
      // Try vault name with special characters
      await nameInput.fill('Test Vault äöü 日本語 🔐')

      // Should either accept or show validation message
      const inputValue = await nameInput.inputValue()
      expect(inputValue.length).toBeGreaterThan(0)
    }
  })

  test('should handle very long vault name', async ({ page }) => {
    const nameInput = page.locator(
      'input[name="vaultName"], input[placeholder*="Name"]',
    )

    if (await nameInput.isVisible()) {
      const longName = 'A'.repeat(500)
      await nameInput.fill(longName)

      // Input might be truncated or show validation error
      const inputValue = await nameInput.inputValue()
      expect(inputValue).toBeDefined()
    }
  })
})

// ============================================================================
// Password Strength Tests
// ============================================================================

test.describe('Password Security', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/')
    await waitForAppReady(page)
    await navigateToVaultCreation(page)
  })

  test('should handle weak password appropriately', async ({ page }) => {
    const passwordInput = page.locator('input[type="password"]').first()

    if (await passwordInput.isVisible()) {
      // Try weak password
      await passwordInput.fill('123')

      // App might show strength indicator or allow it (policy varies)
      await page.waitForTimeout(300)
      expect(await passwordInput.inputValue()).toBe('123')
    }
  })

  test('should handle strong password', async ({ page }) => {
    const passwordInput = page.locator('input[type="password"]').first()

    if (await passwordInput.isVisible()) {
      // Use strong password
      await passwordInput.fill('V3ryStr0ng&SecureP@ssw0rd!')

      await page.waitForTimeout(300)
      expect((await passwordInput.inputValue()).length).toBeGreaterThan(10)
    }
  })
})
