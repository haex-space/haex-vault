# Research: Vault Identity Copy Icon

## Decision: Use haex-ui's `UiButtonCopy`

Use the pinned haex-ui component with the existing localized copy and copied labels and the public key as its value. Forward its `error` event to the existing error toast and retain `data-testid="settings-identity-copy"`.

**Rationale**: `UiButtonIcon` is documented as the base for icon buttons that have no component of their own; copying has one, and every other copy control in the app uses it. It shows the copy icon, switches to a check with the copied name for a moment, and reports clipboard failures through `error`. The success feedback is the check, so no success toast is needed.

**Alternatives considered**:

- `UiButtonIcon` with a local clipboard handler (the earlier decision): rejected because it drops the check feedback and the hover/tooltip behavior of the other copy buttons. Its concern that the legacy clipboard fallback of `UiButtonCopy` could report a false success is unverified; the webview uses the async Clipboard API, and a refused write reaches the `error` toast.
- Letting the accessible name stay fixed while the check shows: not required by the spaex constitution (no such rule), and the component's name change is the same on every other copy control.
