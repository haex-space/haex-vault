# Feature Specification: Vault Identity Copy Icon

**Feature Branch**: `fix/vault-identity-copy-icon`

**Created**: 2026-11-26

**Status**: Draft

**Input**: User request: Replace the “Kopieren” button in the Vault identity section with an icon button, consistent with other app controls.

## User Scenarios & Testing _(mandatory)_

### User Story 1 - Copy the vault identity (Priority: P1)

As a user viewing my vault's public identity, I want a compact copy icon control beside it, so this action matches the app's established icon-button pattern and takes less visual space.

**Why this priority**: This is the requested visual improvement to an existing action.

**Independent Test**: Open the Federation settings and verify that the public identity row shows haex-ui's icon-only copy control; activate it and confirm the public key is copied, the button shows its check, and clipboard failures produce an error toast.

**Acceptance Scenarios**:

1. **Given** the Federation settings show a vault public identity, **When** the identity row is displayed, **Then** its copy action is an icon-only button consistent with other app icon buttons and has an accessible name describing the copy action.
2. **Given** the copy control is available, **When** the user activates it, **Then** the displayed public key is copied and the button shows its check.
3. **Given** clipboard access fails, **When** the user activates the copy control, **Then** an error toast explains the failure.

### Edge Cases

- The button remains operable and accessible when the settings view is used with keyboard navigation or assistive technology.
- Clipboard failure continues to provide the existing error feedback.

## Requirements _(mandatory)_

### Functional Requirements

- **FR-001**: The Vault identity section MUST present its copy action as an icon-only button rather than a text-labeled button.
- **FR-002**: The icon button MUST provide an accessible name that identifies the copy action.
- **FR-003**: Activating the icon button MUST copy the public key, show the copy button's check as success feedback, and show an error toast if copying fails.
- **FR-004**: The icon button MUST follow the visual and interaction conventions used by other icon buttons in the application.

## Success Criteria _(mandatory)_

### Measurable Outcomes

- **SC-001**: In the Federation settings, users can identify and activate the copy action without a visible text label.
- **SC-002**: The public identity is copied through the control, with the button's check on success and an error toast on clipboard failure.
- **SC-003**: Keyboard and assistive technology users can identify the control by its accessible name and activate it.

## Assumptions

- The copy button confirms success with its check and reports failures with an error toast.
- The app's established copy icon and icon-button styling are the intended visual reference.
