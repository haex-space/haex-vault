# Research: Vault Identity Copy Icon

## Decision: Use haex-ui's `UiButtonIcon`

Use the pinned icon button in the identity row's existing `SettingsRow` slot. Pass the copy icon and the localized action name. Keep `data-testid="settings-identity-copy"` for the vault-identity scenario.

The Clipboard API handler awaits writing the key before it shows the success toast. If writing fails, it shows the existing error toast. The pinned `UiButtonCopy` uses a legacy clipboard fallback that can report success when the copy command returns false. Its success state also changes the accessible name, which spaex requires to stay stable.
