# Data model: Small local models finish what they start

Nothing is stored in the vault. The entities are the measurement's and the turn's.

## Routing record (per request, in the report, new)

- `routed`: the domains of the routing call (empty without routing or when it failed).
- `firstOffer`: the names of the tools the first step offered, in the set's names (`x_mail_<tool>`).
- Written for every request, passed and failed (R2).

## Chain set (analysis file, new)

- Same format as the everyday set (`version`, `mailTools`, `requests`), same fixture world.
- Each request: `id` (`<category>-<lang>-<n>[-s]`), `lang`, `category`, `phrasing`, `text`, `expect` (chains,
  played answers, safety) as in the everyday set.
- Categories as the everyday set; at least 4 requests each for look-up, finishing, deleting and a day's bounds.
- Never used to judge a step's success; only to tune and choose wording (FR-012).

## Value form (per call, new)

- Input: a call's arguments and the tool's input schema.
- For each top-level property whose schema declares a single type:
  - `boolean`: `"true"`/`"false"` in any case become `true`/`false`.
  - `integer` / `number`: a decimal number written as text becomes the number (an integer only when it has no
    fraction).
- Everything else stays as sent. The turned arguments replace the call's arguments before the guard, the dialog and
  the stored call.

## Final tool (per tool, derived)

- A tool is final when its declared effect is destructive (extension tools) or its action deletes (holzi's own
  actions with risk class Risky and a delete effect).
- In a routed offer, final tools of a domain follow its reversible change tools (R8.1, if kept).
- A final tool's description gets holzi's sentence (R8.2, if kept).

## Step (measurement)

- `name`, the lever (R4 to R8), the commit, the chain-set reports, the everyday reports, kept or dropped.
