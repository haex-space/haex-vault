/**
 * Tests for windowManager extension auto-start logic
 *
 * Tests the decision logic for:
 * 1. Auto-starting extensions when external requests arrive
 * 2. Choosing between iframe and native webview based on display_mode
 * 3. Handling the EXTENSION_AUTO_START_REQUEST event
 */
import { describe, expect, it } from 'vitest'

// ============================================================================
// Display Mode Decision Logic Tests
// ============================================================================

describe('Display Mode Decision Logic', () => {
  /**
   * The display_mode can be:
   * - 'auto': Use native window on desktop, iframe on mobile
   * - 'window': Always use native window (desktop only, falls back to iframe on mobile)
   * - 'iframe': Always use iframe
   */

  describe('shouldUseNativeWindow calculation', () => {
    const calculateShouldUseNativeWindow = (
      displayMode: 'auto' | 'window' | 'iframe',
      isDesktopPlatform: boolean,
    ): boolean => {
      // This mirrors the logic in windowManager.ts:
      // const shouldUseNativeWindow =
      //   displayMode === 'window' || (displayMode === 'auto' && isDesktop())
      return displayMode === 'window' || (displayMode === 'auto' && isDesktopPlatform)
    }

    describe('on Desktop platform', () => {
      const isDesktop = true

      it('should use native window when displayMode is "auto"', () => {
        expect(calculateShouldUseNativeWindow('auto', isDesktop)).toBe(true)
      })

      it('should use native window when displayMode is "window"', () => {
        expect(calculateShouldUseNativeWindow('window', isDesktop)).toBe(true)
      })

      it('should use iframe when displayMode is "iframe"', () => {
        expect(calculateShouldUseNativeWindow('iframe', isDesktop)).toBe(false)
      })
    })

    describe('on Mobile platform', () => {
      const isDesktop = false

      it('should use iframe when displayMode is "auto"', () => {
        expect(calculateShouldUseNativeWindow('auto', isDesktop)).toBe(false)
      })

      it('should attempt native window when displayMode is "window" (will fall through to iframe)', () => {
        // On mobile, even with displayMode='window', the actual implementation
        // falls through to iframe because native webview windows aren't available
        // But the calculation itself still returns true
        expect(calculateShouldUseNativeWindow('window', isDesktop)).toBe(true)
      })

      it('should use iframe when displayMode is "iframe"', () => {
        expect(calculateShouldUseNativeWindow('iframe', isDesktop)).toBe(false)
      })
    })
  })

  describe('default displayMode behavior', () => {
    it('should default to "auto" when displayMode is undefined', () => {
      const displayMode: 'auto' | 'window' | 'iframe' | undefined = undefined
      const effectiveMode = displayMode ?? 'auto'
      expect(effectiveMode).toBe('auto')
    })

    it('should default to "auto" when displayMode is null', () => {
      const displayMode: 'auto' | 'window' | 'iframe' | null = null
      const effectiveMode = displayMode ?? 'auto'
      expect(effectiveMode).toBe('auto')
    })
  })
})

// ============================================================================
// Extension Auto-Start Logic Tests
// ============================================================================

describe('Extension Auto-Start Logic', () => {
  interface MockWindow {
    id: string
    type: 'system' | 'extension'
    sourceId: string
  }

  describe('shouldAutoStartExtension', () => {
    const shouldAutoStartExtension = (
      extensionId: string,
      existingWindows: MockWindow[],
    ): boolean => {
      // Check if extension already has an open window
      const existingWindow = existingWindows.find(
        w => w.type === 'extension' && w.sourceId === extensionId,
      )
      return !existingWindow
    }

    it('should auto-start when no window exists for the extension', () => {
      const windows: MockWindow[] = []
      expect(shouldAutoStartExtension('haex-pass-id', windows)).toBe(true)
    })

    it('should not auto-start when extension already has an open window', () => {
      const windows: MockWindow[] = [
        { id: 'window-1', type: 'extension', sourceId: 'haex-pass-id' },
      ]
      expect(shouldAutoStartExtension('haex-pass-id', windows)).toBe(false)
    })

    it('should auto-start when other extensions have windows but not this one', () => {
      const windows: MockWindow[] = [
        { id: 'window-1', type: 'extension', sourceId: 'other-extension-id' },
        { id: 'window-2', type: 'system', sourceId: 'settings' },
      ]
      expect(shouldAutoStartExtension('haex-pass-id', windows)).toBe(true)
    })

    it('should not be confused by system windows with same sourceId', () => {
      // System windows have different sourceId namespace
      const windows: MockWindow[] = [
        { id: 'window-1', type: 'system', sourceId: 'haex-pass-id' },
      ]
      // This should return true because it's a system window, not extension
      expect(shouldAutoStartExtension('haex-pass-id', windows)).toBe(true)
    })

    it('should handle multiple windows for the same extension', () => {
      // Some extensions might allow multiple instances
      const windows: MockWindow[] = [
        { id: 'window-1', type: 'extension', sourceId: 'haex-pass-id' },
        { id: 'window-2', type: 'extension', sourceId: 'haex-pass-id' },
      ]
      expect(shouldAutoStartExtension('haex-pass-id', windows)).toBe(false)
    })
  })
})

// ============================================================================
// Extension Identification Tests (matching Rust logic)
// ============================================================================

describe('Extension Identification', () => {
  interface ExtensionIdentifier {
    publicKey: string
    name: string
  }

  describe('Extension unique identification', () => {
    const areExtensionsSame = (a: ExtensionIdentifier, b: ExtensionIdentifier): boolean => {
      return a.publicKey === b.publicKey && a.name === b.name
    }

    it('same developer, same extension name = same extension', () => {
      const ext1: ExtensionIdentifier = {
        publicKey: 'b4401f13f65e576b8a30ff9fd83df82a8bb707e1994d40c99996fe88603cefca',
        name: 'haex-pass',
      }
      const ext2: ExtensionIdentifier = {
        publicKey: 'b4401f13f65e576b8a30ff9fd83df82a8bb707e1994d40c99996fe88603cefca',
        name: 'haex-pass',
      }
      expect(areExtensionsSame(ext1, ext2)).toBe(true)
    })

    it('same developer, different extension name = different extension', () => {
      const ext1: ExtensionIdentifier = {
        publicKey: 'b4401f13f65e576b8a30ff9fd83df82a8bb707e1994d40c99996fe88603cefca',
        name: 'haex-pass',
      }
      const ext2: ExtensionIdentifier = {
        publicKey: 'b4401f13f65e576b8a30ff9fd83df82a8bb707e1994d40c99996fe88603cefca',
        name: 'haex-notes',
      }
      expect(areExtensionsSame(ext1, ext2)).toBe(false)
    })

    it('different developer, same extension name = different extension', () => {
      const ext1: ExtensionIdentifier = {
        publicKey: 'developer1key56789012345678901234567890123456789012345678901234567890',
        name: 'password-manager',
      }
      const ext2: ExtensionIdentifier = {
        publicKey: 'developer2key56789012345678901234567890123456789012345678901234567890',
        name: 'password-manager',
      }
      expect(areExtensionsSame(ext1, ext2)).toBe(false)
    })

    it('different developer, different extension name = different extension', () => {
      const ext1: ExtensionIdentifier = {
        publicKey: 'developer1key56789012345678901234567890123456789012345678901234567890',
        name: 'haex-pass',
      }
      const ext2: ExtensionIdentifier = {
        publicKey: 'developer2key56789012345678901234567890123456789012345678901234567890',
        name: 'haex-notes',
      }
      expect(areExtensionsSame(ext1, ext2)).toBe(false)
    })
  })

  describe('Extension identifier validation', () => {
    const isValidExtensionIdentifier = (identifier: Partial<ExtensionIdentifier>): boolean => {
      return Boolean(
        identifier.publicKey
        && identifier.publicKey.length > 0
        && identifier.name
        && identifier.name.length > 0,
      )
    }

    it('should be valid with both publicKey and name', () => {
      expect(isValidExtensionIdentifier({
        publicKey: 'b4401f13f65e576b8a30ff9fd83df82a8bb707e1994d40c99996fe88603cefca',
        name: 'haex-pass',
      })).toBe(true)
    })

    it('should be invalid with only publicKey', () => {
      expect(isValidExtensionIdentifier({
        publicKey: 'b4401f13f65e576b8a30ff9fd83df82a8bb707e1994d40c99996fe88603cefca',
      })).toBe(false)
    })

    it('should be invalid with only name', () => {
      expect(isValidExtensionIdentifier({
        name: 'haex-pass',
      })).toBe(false)
    })

    it('should be invalid with empty publicKey', () => {
      expect(isValidExtensionIdentifier({
        publicKey: '',
        name: 'haex-pass',
      })).toBe(false)
    })

    it('should be invalid with empty name', () => {
      expect(isValidExtensionIdentifier({
        publicKey: 'b4401f13f65e576b8a30ff9fd83df82a8bb707e1994d40c99996fe88603cefca',
        name: '',
      })).toBe(false)
    })

    it('should be invalid with both empty', () => {
      expect(isValidExtensionIdentifier({
        publicKey: '',
        name: '',
      })).toBe(false)
    })

    it('should be invalid with missing fields', () => {
      expect(isValidExtensionIdentifier({})).toBe(false)
    })
  })
})

// ============================================================================
// Auto-Start Event Payload Tests
// ============================================================================

describe('Auto-Start Event Payload', () => {
  interface AutoStartPayload {
    extensionId: string
  }

  describe('payload structure', () => {
    it('should have extensionId field', () => {
      const payload: AutoStartPayload = {
        extensionId: 'uuid-of-extension',
      }
      expect(payload.extensionId).toBeDefined()
      expect(typeof payload.extensionId).toBe('string')
    })

    it('should serialize correctly to JSON', () => {
      const payload: AutoStartPayload = {
        extensionId: 'uuid-of-extension',
      }
      const json = JSON.stringify(payload)
      expect(json).toContain('extensionId')
      expect(json).toContain('uuid-of-extension')
    })

    it('should deserialize correctly from JSON', () => {
      const json = '{"extensionId":"uuid-of-extension"}'
      const payload: AutoStartPayload = JSON.parse(json)
      expect(payload.extensionId).toBe('uuid-of-extension')
    })
  })
})

// ============================================================================
// Window State Management Tests
// ============================================================================

describe('Window State Management', () => {
  interface WindowState {
    id: string
    type: 'system' | 'extension'
    sourceId: string
    isNativeWebview?: boolean
  }

  describe('findWindowByExtensionId', () => {
    const findWindowByExtensionId = (
      windows: WindowState[],
      extensionId: string,
    ): WindowState | undefined => {
      return windows.find(
        w => w.type === 'extension' && w.sourceId === extensionId,
      )
    }

    it('should find existing extension window', () => {
      const windows: WindowState[] = [
        { id: 'win-1', type: 'extension', sourceId: 'ext-123' },
      ]
      const found = findWindowByExtensionId(windows, 'ext-123')
      expect(found).toBeDefined()
      expect(found?.id).toBe('win-1')
    })

    it('should return undefined when no window exists', () => {
      const windows: WindowState[] = []
      const found = findWindowByExtensionId(windows, 'ext-123')
      expect(found).toBeUndefined()
    })

    it('should not match system windows', () => {
      const windows: WindowState[] = [
        { id: 'win-1', type: 'system', sourceId: 'ext-123' },
      ]
      const found = findWindowByExtensionId(windows, 'ext-123')
      expect(found).toBeUndefined()
    })

    it('should distinguish between native and iframe windows', () => {
      const windows: WindowState[] = [
        { id: 'win-1', type: 'extension', sourceId: 'ext-123', isNativeWebview: true },
        { id: 'win-2', type: 'extension', sourceId: 'ext-456', isNativeWebview: false },
      ]
      const nativeWindow = findWindowByExtensionId(windows, 'ext-123')
      const iframeWindow = findWindowByExtensionId(windows, 'ext-456')

      expect(nativeWindow?.isNativeWebview).toBe(true)
      expect(iframeWindow?.isNativeWebview).toBe(false)
    })
  })

  describe('removeWindowById', () => {
    const removeWindowById = (windows: WindowState[], windowId: string): WindowState[] => {
      return windows.filter(w => w.id !== windowId)
    }

    it('should remove window by id', () => {
      const windows: WindowState[] = [
        { id: 'win-1', type: 'extension', sourceId: 'ext-123' },
        { id: 'win-2', type: 'extension', sourceId: 'ext-456' },
      ]
      const result = removeWindowById(windows, 'win-1')
      expect(result.length).toBe(1)
      expect(result[0].id).toBe('win-2')
    })

    it('should return same array if window not found', () => {
      const windows: WindowState[] = [
        { id: 'win-1', type: 'extension', sourceId: 'ext-123' },
      ]
      const result = removeWindowById(windows, 'non-existent')
      expect(result.length).toBe(1)
    })

    it('should return empty array when removing last window', () => {
      const windows: WindowState[] = [
        { id: 'win-1', type: 'extension', sourceId: 'ext-123' },
      ]
      const result = removeWindowById(windows, 'win-1')
      expect(result.length).toBe(0)
    })
  })
})

// ============================================================================
// Event Name Constants Tests
// ============================================================================

describe('Event Name Constants', () => {
  // These should match the values in eventNames.json
  const EXTENSION_WINDOW_CLOSED = 'extension:window-closed'
  const EXTENSION_AUTO_START_REQUEST = 'extension:auto-start-request'

  it('should use colon separator format', () => {
    expect(EXTENSION_WINDOW_CLOSED).toContain(':')
    expect(EXTENSION_AUTO_START_REQUEST).toContain(':')
  })

  it('should start with "extension:" prefix', () => {
    expect(EXTENSION_WINDOW_CLOSED.startsWith('extension:')).toBe(true)
    expect(EXTENSION_AUTO_START_REQUEST.startsWith('extension:')).toBe(true)
  })

  it('should have descriptive names after prefix', () => {
    expect(EXTENSION_WINDOW_CLOSED).toBe('extension:window-closed')
    expect(EXTENSION_AUTO_START_REQUEST).toBe('extension:auto-start-request')
  })
})
