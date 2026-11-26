# Research: Vault Identity Copy Icon

## Decision: Extend the existing identity row

The control already lives in `src/components/settings/VaultIdentityRow.vue`, and `SettingsRow` provides the right-side control slot. Change only the button presentation there.

**Rationale**: The requested behavior is specific to the existing row; a new component or abstraction would add unnecessary surface area.

**Alternatives considered**: A shared copy-button component was rejected because this row already owns the copy behavior and no shared component is needed to achieve the requested consistency.

## Decision: Use haex-ui's `UiButtonCopy`

Use the pinned haex-ui component with the existing localized copy and copied labels and the public key as its value.

**Rationale**: The existing layer provides the standard icon-only copy control, its accessible label and copied feedback. Existing call sites use this component for the same action.

**Alternatives considered**: Rebuilding the icon button locally with `UiButton` was rejected because it would duplicate an existing haex-ui primitive.

## Decision: Preserve the current operation

Let `UiButtonCopy` copy the value and show its localized copied feedback. Forward the component's error event to the existing error toast and retain `data-testid="settings-identity-copy"`.

**Rationale**: This matches established project usage while preserving visible clipboard failure feedback.
