/**
 * Cross-language constants synchronization test
 *
 * This test verifies that the VaultSettings constants in TypeScript
 * match the constants defined in Rust (src-tauri/src/database/constants.rs).
 *
 * If this test fails, it means the constants are out of sync between
 * TypeScript and Rust. Update both files to match.
 */

import { describe, it, expect } from 'vitest'
import {
  VaultSettingsTypeEnum,
  VaultSettingsKeyEnum,
} from '../../src/config/vault-settings'

describe('VaultSettings Constants Synchronization', () => {
  describe('VaultSettingsTypeEnum', () => {
    // These values must match Rust: src-tauri/src/database/constants.rs::vault_settings_type
    it('should have correct "settings" value', () => {
      expect(VaultSettingsTypeEnum.settings).toBe('settings')
    })

    it('should have correct "system" value', () => {
      expect(VaultSettingsTypeEnum.system).toBe('system')
    })
  })

  describe('VaultSettingsKeyEnum', () => {
    // These values must match Rust: src-tauri/src/database/constants.rs::vault_settings_key
    // All values should be snake_case

    it('should have correct "locale" value', () => {
      expect(VaultSettingsKeyEnum.locale).toBe('locale')
    })

    it('should have correct "theme" value', () => {
      expect(VaultSettingsKeyEnum.theme).toBe('theme')
    })

    it('should have correct "vaultName" value (snake_case)', () => {
      expect(VaultSettingsKeyEnum.vaultName).toBe('vault_name')
    })

    it('should have correct "vaultId" value (snake_case)', () => {
      expect(VaultSettingsKeyEnum.vaultId).toBe('vault_id')
    })

    it('should have correct "desktopIconSize" value (snake_case)', () => {
      expect(VaultSettingsKeyEnum.desktopIconSize).toBe('desktop_icon_size')
    })

    it('should have correct "tombstoneRetentionDays" value (snake_case)', () => {
      expect(VaultSettingsKeyEnum.tombstoneRetentionDays).toBe('tombstone_retention_days')
    })

    it('should have correct "externalBridgePort" value (snake_case)', () => {
      expect(VaultSettingsKeyEnum.externalBridgePort).toBe('external_bridge_port')
    })

    it('should have correct "initialSyncComplete" value (snake_case)', () => {
      expect(VaultSettingsKeyEnum.initialSyncComplete).toBe('initial_sync_complete')
    })

    it('should have correct "gradientVariant" value (snake_case)', () => {
      expect(VaultSettingsKeyEnum.gradientVariant).toBe('gradient_variant')
    })

    it('should have correct "gradientEnabled" value (snake_case)', () => {
      expect(VaultSettingsKeyEnum.gradientEnabled).toBe('gradient_enabled')
    })
  })

  describe('All values use snake_case', () => {
    it('should have all VaultSettingsKeyEnum values in snake_case', () => {
      const values = Object.values(VaultSettingsKeyEnum)
      for (const value of values) {
        // snake_case: lowercase with underscores, no uppercase letters
        expect(value).toMatch(/^[a-z][a-z0-9_]*$/)
      }
    })
  })
})
