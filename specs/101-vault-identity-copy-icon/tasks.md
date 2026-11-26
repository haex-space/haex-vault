# Tasks: Vault Identity Copy Icon

**Input**: Design documents from `specs/101-vault-identity-copy-icon/`

**Prerequisites**: `plan.md`, `spec.md`, `research.md`

## Phase 1: Setup

**Purpose**: Confirm the existing haex-ui icon button and translation strings.

- [x] T001 Confirm `UiButtonIcon` usage and existing identity copy strings in `src/components/settings/VaultIdentityRow.vue`, neighboring components and `src/i18n/locales/de.json` / `src/i18n/locales/en.json`

## Phase 2: User Story 1 - Copy the vault identity (Priority: P1)

**Independent Test**: In Federation settings, the vault identity row exposes an icon-only copy action with an accessible localized label; activating it copies the key and shows a localized success toast. Clipboard failures still show an error toast.

- [x] T002 [US1] Replace the text `UiButton` with haex-ui `UiButtonIcon` in `src/components/settings/VaultIdentityRow.vue`. Use the copy icon, localized accessible name, `size="icon-sm"`, and existing test identifier. Keep the clipboard handler and its toasts.

## Dependencies

- T002 depends on T001's confirmation of the component API and available localized strings.

## Parallel Opportunities

- No parallel implementation tasks; the story is a single component change.

## Implementation Strategy

Deliver User Story 1 as the complete MVP. The existing `sync-identity` scenario verifies the copy toast.
