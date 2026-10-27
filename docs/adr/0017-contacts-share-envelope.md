# Contacts are shared through an envelope

Status: proposed
Date: 2026-10-26

## Context

Spec 051's own-card flow cannot express per-share selection and treats the signed card as the
whole contact. Spec 060 needs the same sharing dialog for the own contact and every address-book
contact, while preserving the signed proof for holzi addresses and ensuring that importing a
partial contact never removes existing data.

## Decision

Spec 060 uses a versioned `ShareEnvelope` encoded as a `haex-vault://contact/…` deep link. The
link, clipboard text and QR code carry exactly the same text. Contact fields are independently
selectable; a foreign contact's signed card is embedded unchanged, while the own contact gets a
freshly signed card containing only the selected devices and relays. Opening a share only shows a
preview. Import requires an explicit per-item decision, is add-only by default, never replaces a
confirmed vault identity, and rejects a stale existing-contact preview before writing.

Signed cards are stored by a hash of their canonical payload, so several cards from one vault can
coexist and the verified addresses are derived from the stored cards. Migration 0043 rebuilds that
table and intentionally drops the old rows; `HOLZI_TRIGGER_VERSION` becomes 28. Desktop processes
hand off deep links through `<AppLocalData>/link-inbox/`. Agent sharing is a destructive action
with a fresh user confirmation for every call.

## Consequences

- A share cannot accidentally remove existing contact data, but users must explicitly choose an
  update when they want to replace a value.
- Existing confirmed addresses are lost during the table rebuild and must be re-imported; this is
  a deliberate migration trade-off and is covered by the implementation plan.
- Links can be previewed without changing the vault, and stale previews cannot overwrite a newer
  contact revision.
- The envelope and inbox add format and process-handoff code, with limits and cleanup rules that
  must be tested on each supported platform.

## Alternatives

- Keep using the old own-card text: it cannot represent per-share field selection.
- Use vCard: deferred to a later feature and it does not carry the signed holzi-address proof.
- Enforce one process with a single-instance plugin: rejected because one process per vault session
  is an existing design constraint.
- Use random card-row IDs: rejected because the same signed card would be duplicated after import;
  the canonical-payload hash is deterministic across devices.
