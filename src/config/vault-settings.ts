/**
 * Vault settings constants
 *
 * IMPORTANT: These values must match the Rust constants in src-tauri/src/database/constants.rs
 * Run `pnpm test:constants` to verify synchronization
 */

// Vault settings key values (all snake_case for consistency)
export enum VaultSettingsKeyEnum {
  locale = 'locale',
  theme = 'theme',
  vaultName = 'vault_name',
  spaceId = 'space_id',
  desktopIconSize = 'desktop_icon_size',
  tombstoneRetentionDays = 'tombstone_retention_days',
  externalBridgePort = 'external_bridge_port',
  initialSyncComplete = 'initial_sync_complete',
  gradientVariant = 'gradient_variant',
  gradientEnabled = 'gradient_enabled',
  onboardingCompleted = 'onboarding_completed',
  peerStorageAutostart = 'peer_storage_autostart',
  peerStorageRelayUrl = 'peer_storage_relay_url',
  logRetentionDays = 'log_retention_days',
  localDsMessageTtlDays = 'local_ds_message_ttl_days',
  localDsKeyPackageTtlHours = 'local_ds_key_package_ttl_hours',
  localDsWelcomeTtlDays = 'local_ds_welcome_ttl_days',
  localDsPendingCommitTtlHours = 'local_ds_pending_commit_ttl_hours',
  localDsCleanupIntervalMinutes = 'local_ds_cleanup_interval_minutes',
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
