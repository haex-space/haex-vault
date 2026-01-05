/**
 * Vault settings constants
 *
 * IMPORTANT: These values must match the Rust constants in src-tauri/src/database/constants.rs
 * Run `pnpm test:constants` to verify synchronization
 */

// Vault settings type values
export enum VaultSettingsTypeEnum {
  settings = 'settings',
  system = 'system',
}

// Vault settings key values (all snake_case for consistency)
export enum VaultSettingsKeyEnum {
  locale = 'locale',
  theme = 'theme',
  vaultName = 'vault_name',
  vaultId = 'vault_id',
  desktopIconSize = 'desktop_icon_size',
  tombstoneRetentionDays = 'tombstone_retention_days',
  externalBridgePort = 'external_bridge_port',
  initialSyncComplete = 'initial_sync_complete',
  gradientVariant = 'gradient_variant',
  gradientEnabled = 'gradient_enabled',
}

export enum DesktopIconSizePreset {
  small = 'small',
  medium = 'medium',
  large = 'large',
  extraLarge = 'extra-large',
}

export const iconSizePresetValues: Record<DesktopIconSizePreset, number> = {
  [DesktopIconSizePreset.small]: 60,
  [DesktopIconSizePreset.medium]: 80,
  [DesktopIconSizePreset.large]: 120,
  [DesktopIconSizePreset.extraLarge]: 160,
}
