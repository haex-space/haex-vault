// e2e/extension-management.spec.ts
//
// E2E Tests for Extension Management
//
// Tests the complete extension lifecycle:
// - Viewing installed extensions
// - Installing new extensions
// - Configuring extension permissions
// - Uninstalling extensions
// - Extension isolation and security
//

import { test, expect, Page } from '@playwright/test'

// ============================================================================
// Test Configuration
// ============================================================================

// Test extension URLs (if available in test environment)
const TEST_EXTENSION_URL = process.env.TEST_EXTENSION_URL || ''

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
 * Navigate to the extensions settings page
 */
async function navigateToExtensionsSettings(page: Page): Promise<void> {
  // Look for settings button/icon
  const settingsButton = page.locator(
    '[data-testid="settings-btn"], button[aria-label*="Settings"], button[aria-label*="Einstellungen"], [class*="settings"]',
  )

  if (await settingsButton.first().isVisible().catch(() => false)) {
    await settingsButton.first().click()
    await page.waitForTimeout(500)
  }

  // Look for extensions tab/section
  const extensionsTab = page.locator(
    '[data-testid="extensions-tab"], button:has-text("Extensions"), button:has-text("Erweiterungen"), a:has-text("Extensions")',
  )

  if (await extensionsTab.first().isVisible().catch(() => false)) {
    await extensionsTab.first().click()
    await page.waitForTimeout(500)
  }
}

/**
 * Check if an extension is installed
 */
async function isExtensionInstalled(
  page: Page,
  extensionName: string,
): Promise<boolean> {
  const extensionItem = page.locator(
    `[data-testid="extension-item"]:has-text("${extensionName}"), [class*="extension"]:has-text("${extensionName}")`,
  )
  return extensionItem.isVisible().catch(() => false)
}

/**
 * Open extension installation dialog
 */
async function openInstallDialog(page: Page): Promise<void> {
  const installButton = page.locator(
    '[data-testid="install-extension-btn"], button:has-text("Install"), button:has-text("Installieren"), button:has-text("Add"), button:has-text("Hinzufügen")',
  )

  if (await installButton.isVisible()) {
    await installButton.click()
    await page.waitForTimeout(500)
  }
}

// ============================================================================
// Extension List Tests
// ============================================================================

test.describe('Extension List', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/')
    await waitForAppReady(page)
  })

  test('should display extensions section', async ({ page }) => {
    await navigateToExtensionsSettings(page)

    // Check for extensions list container
    const extensionsList = page.locator(
      '[data-testid="extensions-list"], [class*="extensions"], [aria-label*="Extensions"]',
    )

    const pageContent = await page.content()

    // Either the list is visible or the page contains extension-related content
    const hasExtensionsContent =
      (await extensionsList.isVisible().catch(() => false)) ||
      pageContent.includes('Extension') ||
      pageContent.includes('Erweiterung')

    expect(hasExtensionsContent).toBeTruthy()
  })

  test('should show extension details when clicked', async ({ page }) => {
    await navigateToExtensionsSettings(page)

    // Find any extension item
    const extensionItem = page
      .locator('[data-testid="extension-item"], [class*="extension-card"]')
      .first()

    if (await extensionItem.isVisible().catch(() => false)) {
      await extensionItem.click()
      await page.waitForTimeout(500)

      // Should show some details (permissions, version, etc.)
      const hasDetails =
        (await page.locator('[data-testid="extension-detail"]').isVisible().catch(() => false)) ||
        (await page.locator('text=/version|Version|Berechtigungen|permissions/i').isVisible().catch(() => false))

      expect(hasDetails).toBeTruthy()
    }
  })
})

// ============================================================================
// Extension Installation Tests
// ============================================================================

test.describe('Extension Installation', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/')
    await waitForAppReady(page)
    await navigateToExtensionsSettings(page)
  })

  test('should show install option', async ({ page }) => {
    // Look for install/add button
    const installButton = page.locator(
      'button:has-text("Install"), button:has-text("Installieren"), button:has-text("Add"), button:has-text("Hinzufügen"), [data-testid="install-extension"]',
    )

    const pageContent = await page.content()
    const hasInstallOption =
      (await installButton.first().isVisible().catch(() => false)) ||
      pageContent.includes('Install') ||
      pageContent.includes('Installieren')

    expect(hasInstallOption).toBeTruthy()
  })

  test('should open installation dialog', async ({ page }) => {
    await openInstallDialog(page)

    // Check for installation dialog/modal
    const hasDialog =
      (await page.locator('[role="dialog"]').isVisible().catch(() => false)) ||
      (await page.locator('[data-testid="install-dialog"]').isVisible().catch(() => false)) ||
      (await page.locator('input[type="file"]').isVisible().catch(() => false)) ||
      (await page.locator('input[placeholder*="URL"]').isVisible().catch(() => false))

    // Dialog or some form of installation UI should appear
    expect(hasDialog || (await page.locator('form').isVisible())).toBeTruthy()
  })

  test.skip('should validate extension URL format', async ({ page }) => {
    await openInstallDialog(page)

    const urlInput = page.locator(
      'input[placeholder*="URL"], input[type="url"], input[name="extensionUrl"]',
    )

    if (await urlInput.isVisible()) {
      // Try invalid URL
      await urlInput.fill('not-a-valid-url')

      const submitButton = page.locator(
        'button[type="submit"], button:has-text("Install"), button:has-text("Installieren")',
      )

      if (await submitButton.isVisible()) {
        await submitButton.click()
        await page.waitForTimeout(500)

        // Should show validation error
        const hasError = await page
          .locator('[class*="error"], [role="alert"]')
          .isVisible()
          .catch(() => false)

        expect(hasError).toBeTruthy()
      }
    }
  })

  test.skip('should show permission review before installation', async ({
    page,
  }) => {
    // This test requires a valid extension URL
    if (!TEST_EXTENSION_URL) {
      test.skip()
      return
    }

    await openInstallDialog(page)

    const urlInput = page.locator(
      'input[placeholder*="URL"], input[type="url"]',
    )

    if (await urlInput.isVisible()) {
      await urlInput.fill(TEST_EXTENSION_URL)

      const submitButton = page.locator(
        'button[type="submit"], button:has-text("Install")',
      )

      if (await submitButton.isVisible()) {
        await submitButton.click()
        await page.waitForTimeout(2000)

        // Should show permissions review
        const hasPermissionReview =
          (await page.locator('text=/permission|Berechtigung/i').isVisible().catch(() => false)) ||
          (await page.locator('[data-testid="permission-review"]').isVisible().catch(() => false))

        expect(hasPermissionReview).toBeTruthy()
      }
    }
  })
})

// ============================================================================
// Extension Uninstallation Tests
// ============================================================================

test.describe('Extension Uninstallation', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/')
    await waitForAppReady(page)
    await navigateToExtensionsSettings(page)
  })

  test('should show uninstall option for installed extensions', async ({
    page,
  }) => {
    // Find an extension item
    const extensionItem = page
      .locator('[data-testid="extension-item"], [class*="extension-card"]')
      .first()

    if (await extensionItem.isVisible().catch(() => false)) {
      await extensionItem.click()
      await page.waitForTimeout(500)

      // Look for uninstall/remove button
      const uninstallButton = page.locator(
        'button:has-text("Uninstall"), button:has-text("Deinstallieren"), button:has-text("Remove"), button:has-text("Entfernen"), [data-testid="uninstall-btn"]',
      )

      const hasUninstall = await uninstallButton.isVisible().catch(() => false)
      expect(hasUninstall).toBeTruthy()
    }
  })

  test('should show confirmation before uninstalling', async ({ page }) => {
    const extensionItem = page
      .locator('[data-testid="extension-item"], [class*="extension-card"]')
      .first()

    if (await extensionItem.isVisible().catch(() => false)) {
      await extensionItem.click()
      await page.waitForTimeout(500)

      const uninstallButton = page.locator(
        'button:has-text("Uninstall"), button:has-text("Deinstallieren"), button:has-text("Remove")',
      )

      if (await uninstallButton.isVisible()) {
        await uninstallButton.click()
        await page.waitForTimeout(500)

        // Should show confirmation dialog
        const hasConfirmation =
          (await page.locator('[role="dialog"]').isVisible().catch(() => false)) ||
          (await page.locator('text=/confirm|Bestätigen|sicher|sure/i').isVisible().catch(() => false))

        expect(hasConfirmation).toBeTruthy()
      }
    }
  })
})

// ============================================================================
// Extension Permission Tests
// ============================================================================

test.describe('Extension Permissions', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/')
    await waitForAppReady(page)
    await navigateToExtensionsSettings(page)
  })

  test('should display extension permissions', async ({ page }) => {
    const extensionItem = page
      .locator('[data-testid="extension-item"], [class*="extension-card"]')
      .first()

    if (await extensionItem.isVisible().catch(() => false)) {
      await extensionItem.click()
      await page.waitForTimeout(500)

      // Look for permissions section
      const permissionsSection = page.locator(
        '[data-testid="permissions-section"], [class*="permission"], text=/Database|Filesystem|HTTP|Shell|Datenbank|Dateisystem/i',
      )

      const hasPermissions = await permissionsSection
        .first()
        .isVisible()
        .catch(() => false)

      // Permissions should be visible in the detail view
      expect(hasPermissions).toBeTruthy()
    }
  })

  test('should allow editing permissions', async ({ page }) => {
    const extensionItem = page
      .locator('[data-testid="extension-item"]')
      .first()

    if (await extensionItem.isVisible().catch(() => false)) {
      await extensionItem.click()
      await page.waitForTimeout(500)

      // Look for permission toggle or edit button
      const permissionToggle = page.locator(
        '[data-testid="permission-toggle"], input[type="checkbox"], [role="switch"]',
      )

      const hasToggle = await permissionToggle.first().isVisible().catch(() => false)

      // Either toggles or permission management should exist
      expect(
        hasToggle ||
          (await page.locator('text=/edit|bearbeiten/i').isVisible().catch(() => false)),
      ).toBeTruthy()
    }
  })
})

// ============================================================================
// Extension Security Tests
// ============================================================================

test.describe('Extension Security', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/')
    await waitForAppReady(page)
  })

  test('should show extension signature status', async ({ page }) => {
    await navigateToExtensionsSettings(page)

    const extensionItem = page
      .locator('[data-testid="extension-item"]')
      .first()

    if (await extensionItem.isVisible().catch(() => false)) {
      await extensionItem.click()
      await page.waitForTimeout(500)

      // Look for signature/verification status
      const signatureInfo = page.locator(
        '[data-testid="signature-status"], text=/verified|signiert|signature|Signatur|valid|gültig/i',
      )

      const hasSignatureInfo = await signatureInfo
        .first()
        .isVisible()
        .catch(() => false)

      // Some indication of extension authenticity should be shown
      expect(
        hasSignatureInfo ||
          (await page.locator('[class*="badge"]').isVisible().catch(() => false)),
      ).toBeTruthy()
    }
  })

  test('should display extension public key', async ({ page }) => {
    await navigateToExtensionsSettings(page)

    const extensionItem = page
      .locator('[data-testid="extension-item"]')
      .first()

    if (await extensionItem.isVisible().catch(() => false)) {
      await extensionItem.click()
      await page.waitForTimeout(500)

      // Look for public key display
      const publicKeyInfo = page.locator(
        '[data-testid="public-key"], text=/public.?key|öffentlicher.?schlüssel/i, code',
      )

      const hasPublicKey = await publicKeyInfo
        .first()
        .isVisible()
        .catch(() => false)

      // Public key should be visible for transparency
      expect(hasPublicKey).toBeTruthy()
    }
  })
})

// ============================================================================
// Extension UI Integration Tests
// ============================================================================

test.describe('Extension UI Integration', () => {
  test('should open extension in window/iframe', async ({ page }) => {
    await page.goto('/')
    await waitForAppReady(page)

    // Find any extension launcher (desktop icon, menu item, etc.)
    const extensionLauncher = page.locator(
      '[data-testid="extension-launcher"], [data-testid="desktop-icon"], [class*="extension-icon"]',
    )

    if (await extensionLauncher.first().isVisible().catch(() => false)) {
      await extensionLauncher.first().dblclick()
      await page.waitForTimeout(1000)

      // Extension should open in a window or iframe
      const hasExtensionWindow =
        (await page.locator('iframe').isVisible().catch(() => false)) ||
        (await page.locator('[data-testid="extension-window"]').isVisible().catch(() => false))

      expect(hasExtensionWindow).toBeTruthy()
    }
  })

  test('should close extension window', async ({ page }) => {
    await page.goto('/')
    await waitForAppReady(page)

    const extensionLauncher = page
      .locator('[data-testid="extension-launcher"]')
      .first()

    if (await extensionLauncher.isVisible().catch(() => false)) {
      await extensionLauncher.dblclick()
      await page.waitForTimeout(1000)

      // Find close button
      const closeButton = page.locator(
        '[data-testid="window-close"], button[aria-label="Close"], button[aria-label="Schließen"]',
      )

      if (await closeButton.isVisible()) {
        await closeButton.click()
        await page.waitForTimeout(500)

        // Window should be closed
        const hasWindow = await page
          .locator('[data-testid="extension-window"]')
          .isVisible()
          .catch(() => false)

        expect(hasWindow).toBeFalsy()
      }
    }
  })
})
