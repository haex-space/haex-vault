# Tasks: Vault Identity Copy Icon

**Input**: Design documents from `specs/101-vault-identity-copy-icon/`

**Prerequisites**: `plan.md`, `spec.md`, `research.md`

## Phase 1: Setup

**Purpose**: Confirm the existing haex-ui copy primitive and translation strings.

- [x] T001 Confirm `UiButtonCopy` usage and existing identity copy/copied strings in `src/components/settings/VaultIdentityRow.vue`, neighboring components and `src/i18n/locales/de.json` / `src/i18n/locales/en.json`

## Phase 2: User Story 1 - Copy the vault identity (Priority: P1)

**Independent Test**: In Federation settings, the vault identity row exposes an icon-only copy action with an accessible localized label; activating it copies the key and shows copied feedback. Clipboard failures still show an error toast.

- [X] T002 [US1] Replace the text `UiButton` with haex-ui `UiButtonCopy` in `src/components/settings/VaultIdentityRow.vue`; pass `identity.npub`, the localized copy/copied labels, `size="icon-sm"`, and the existing test id; forward copy errors to the existing error toast

## Dependencies

- T002 depends on T001's confirmation of the component API and available localized strings.

## Parallel Opportunities

- No parallel implementation tasks; the story is a single component change.

## Implementation Strategy

Deliver User Story 1 as the complete MVP. No tests were requested for this presentation change.
