# Contract: frontend behavior

The frontend changes are small because the process boundary does the isolation. What remains is what
the user sees, how errors read, and how long a secret lives in the interface.

## Closing the vault (FR-001, FR-002, FR-009)

- The lock control (since 2026-09-30 the last tile of the launcher, `useVaultLock`; before that a
  button on the chat page and on the federation page) flushes the layout, calls `closeAsync()` and
  does nothing else afterwards. The current steps `store.setActiveInstance(null)` and `navigateTo('/')` are
  removed, because the backend replaces the page.
- The frontend keeps **no closing state and shows no overlay** (operator decision 2026-09-21). The
  backend navigates the webview in the same synchronous step that shuts the gate, so the old page,
  with every store and variable it holds, is discarded at once. Between the click and that
  navigation the old content stays visible for the length of one call, which is accepted.
- `closeAsync()` cannot fail by design. If the invoke rejects anyway (the page is already gone, or
  the gate answered `VaultClosed`), the page swallows it: the outcome is the same.
- Double clicks and closing the window during the grace period need no handling: the call is
  idempotent.

## Closing page

A static page, `closing.html`, shown by the backend navigation. It shows **only a spinner and no
text** (operator decision 2026-09-21), so nothing is localized. The spinner is pure CSS in an inline
`<style>` (a rotating ring), with no script, no image or GIF, no external resource and no vault
data. Its background follows `prefers-color-scheme` and matches the app's light and dark
backgrounds, so dark mode gets no white flash; `prefers-reduced-motion` slows the rotation but never
hides the spinner. It lives at `public/closing.html` in the repository root and is served by
`pnpm dev` and present in the built output (research R4, **Outcome**); the fallback is a blank page.

## Errors (`useErrorString`)

| Kind                        | German                                                     | English                                         |
| --------------------------- | ---------------------------------------------------------- | ----------------------------------------------- |
| `VaultAlreadyOpenElsewhere` | "Diese Vault ist in einer anderen Holzi-Instanz geöffnet." | "This vault is open in another Holzi instance." |
| `VaultAlreadyActive`        | "In diesem Fenster ist bereits eine Vault geöffnet."       | "A vault is already open in this window."       |
| `VaultClosed`               | "Die Vault wurde geschlossen."                             | "The vault was closed."                         |

Keys live under `errors.*` in `src/i18n/locales/de.json` and `en.json`. The unlock flow shows
`VaultAlreadyOpenElsewhere`, `WrongPassphrase`, `NotFound`, storage, and compatibility failures
with distinct localized messages; unknown failures use the generic fallback.

## Passphrase lifetime (FR-016)

- `UnlockSheet.vue` and `CreateSheet.vue` keep the field value while the call runs and after a
  failed attempt, so a typo can be corrected without retyping everything. The arguments object is a
  local that goes out of scope with the call.
- The field is set to `''` when the unlock succeeds (before the `unlocked` event is emitted) and when
  the sheet is dismissed (the existing `reset()`). When a close begins the page is discarded, which
  drops the field with everything else, so no explicit clear is needed.
- The value is never written to a Pinia store, `localStorage`, a route parameter or a log line.
- Overwriting the field with random data is deliberately not done. A JavaScript string cannot be
  overwritten in place, so assigning new content only drops the reference and the old string is freed
  without being wiped. The process ending is the guarantee (spec assumptions).
- A replay test asserts the field is cleared after success and after dismissal, kept after a failure,
  and never present in any store's state. Closing is not tested in the frontend: the backend
  discards the page.

## Vault list (FR-021)

`pages/index.vue` already syncs on mount. Additionally it re-syncs on `window` focus and when the
unlock sheet opens, so a vault created by another app process appears without a restart. The
in-process `instance-list-changed` listener stays for the local case.

## Frontend reload recovery

When the webview reloads while Rust still holds an active vault, `pages/index.vue` queries the
app-scoped `active_instance_name` command and navigates back to that vault's workspace. This keeps
the single-vault-per-process rule intact while preventing a second unlock attempt from being
misleadingly shown as a blocked vault.

## What is deliberately not built

- No epoch header, no scoped stores, no per-store reset: a new vault still runs in a new process;
  a webview reload in the existing process recovers its active workspace through the app-scoped
  lifecycle query above.
- No frontend timer for the grace period: the backend owns all deadlines.
