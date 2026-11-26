# Implementation Plan: Vault Identity Copy Icon

**Branch**: `fix/vault-identity-copy-icon` | **Date**: 2026-11-26 | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `specs/101-vault-identity-copy-icon/spec.md`

## Summary

Replace the text button in the Federation vault identity row with haex-ui's established `UiButtonCopy`. Use its localized copy/copied labels, keep clipboard errors visible as a toast, and preserve the existing test identifier.

## Technical Context

**Language/Version**: TypeScript and Vue 3, as used by the existing UI

**Primary Dependencies**: Pinned haex-ui Nuxt layer (`UiButtonCopy`) and Vue I18n

**Storage**: N/A

**Testing**: The existing identity copy test identifier remains in place. No tests were requested for this presentation change.

**Target Platform**: haex-vault desktop application

**Project Type**: Desktop application with Vue frontend and Rust backend

**Performance Goals**: No measurable runtime impact

**Constraints**: Use haex-ui's icon-button sizing and style; pass localized copy/copied labels; retain `settings-identity-copy` for existing automation.

**Scale/Scope**: One control in `src/components/settings/VaultIdentityRow.vue`; no locale changes are needed.

## Constitution Check

*Gate: checked before research and after design.*

- No secrets or local absolute paths are introduced.
- The public identity remains read-only; only its existing copy action changes presentation.
- The copy action uses haex-ui's built-in copied feedback and preserves clipboard-error toast feedback.
- The control remains accessible and follows existing component conventions.
- Work is isolated on the dedicated `fix/vault-identity-copy-icon` topic branch and will land through a pull request.

## Project Structure

### Documentation (this feature)

```text
specs/101-vault-identity-copy-icon/
├── plan.md
├── research.md
├── data-model.md
├── quickstart.md
├── checklists/requirements.md
└── spec.md
```

### Source Code

```text
src/components/settings/VaultIdentityRow.vue  # icon-only copy action
```

**Structure Decision**: Extend the existing settings row; use the already-pinned haex-ui component and existing locale entries. No new UI component or backend code is needed.

## Design Summary

Use `UiButtonCopy` in the current `SettingsRow` slot, matching existing uses in `PathBar.vue`, `BlossomKeySetting.vue` and `DelegateSignInDialog.vue`. Pass `identity.npub` as its value, reuse the existing localized `identityCopy` and `identityCopied` labels, preserve the test id, and route its error event to the existing error toast. The component owns copying and its accessible copy/copied feedback.

## Complexity Tracking

No constitution violations or added architectural complexity.
