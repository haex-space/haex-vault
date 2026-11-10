# Data model: Context for local models

## Context setting (a preference row)

| Field   | Value                                                                                                                                  |
| ------- | -------------------------------------------------------------------------------------------------------------------------------------- |
| Table   | `preferences` (existing)                                                                                                               |
| Scope   | `PrefScope::Device(device uuid)`                                                                                                       |
| Key     | `models.context.<model id>` (the installed model's id, as in `models.id`)                                                              |
| Value   | JSON text `{"context": "auto" \| <integer>, "cache": "standard" \| "q8" \| "q4"}`                                                      |
| Default | no row, or a value that does not parse, or a field that is not one of the above: `auto` and `standard`                                 |
| Rules   | a fixed `context` is an integer from 2048 to `4 × trained length` (no upper bound is checked when the model's file declares no length) |
| Removal | choosing Auto and standard deletes the row; deleting the model deletes its rows (every device's, by key prefix)                        |

A write that breaks a rule is refused with the reason and changes nothing (spec FR-007/FR-008); the page shows it under the field.

## Context in effect (not stored)

Held by the loaded model and read from it; never written.

| Field       | Meaning                                                             |
| ----------- | ------------------------------------------------------------------- |
| `context`   | tokens a generation may use (`Placement::context`)                  |
| `gpuLayers` | layers on the GPU, with the output layer counted as in `Placement`  |
| `layers`    | the model's layers, so the page can say "all" or "12 of 36"         |
| `cache`     | `standard` \| `q8` \| `q4`                                          |
| `yarnFrom`  | `null`, or the training length the model is stretched from          |
| `reason`    | why it is what it is: `target` \| `trained` \| `memory` \| `choice` |

## Model limits (computed, not stored)

From the header and the free memory at the time of the question, for a given cache type.

| Field          | Meaning                                                                                  |
| -------------- | ---------------------------------------------------------------------------------------- |
| `trained`      | the file's `context_length`, or `null`                                                   |
| `maxOnGpu`     | the largest context with every layer on the GPU (at most the target or the fixed choice) |
| `maxAny`       | the largest context that fits with layers on the host                                    |
| `maxStretched` | `4 × trained` where the model can be stretched, else `null`                              |
| `sized`        | `false` when the header does not allow an estimate (spec Edge Cases)                     |

## Summary row (`chat_messages`)

No migration: a summary is a row of the existing table with a new role, so it syncs like any message.

| Column                               | In a summary row                                                        | In every other row |
| ------------------------------------ | ----------------------------------------------------------------------- | ------------------ |
| `role`                               | `summary` (new `MessageRole::Summary`)                                  | unchanged          |
| `content`                            | the summary text, at most about 1200 tokens (chars/3)                   | unchanged          |
| `parent_id`                          | the id of the last row it covers, required                              | the previous row   |
| `created_at`                         | `created_at` of the covered row; the insert makes it one later at least | unchanged          |
| `model_id`, `provider_id`            | the model that wrote it                                                 | unchanged          |
| `prompt_tokens`, `completion_tokens` | empty                                                                   | unchanged          |
| tool columns                         | `NULL` (the existing validator enforces it)                             | unchanged          |

- **Covered rows** are all rows of the thread that sort at or before its parent row, in the list's order
  (`created_at`, `id`), excluding earlier summary rows. They are not changed.
- **Latest covering summary** of a thread: the summary row whose covered row sorts last; ties go to the newest `created_at`, then
  the greater `id`. A row that names a covered row that no longer exists (deleted thread part) is ignored.
- **A summary never covers**: the latest user message of the thread or anything after it; a row that is itself a summary is
  covered only through the text of its successor (R11: the earlier summary is input of the next one).
- **Validation** (`ChatMessageValidationError`): `summary` requires a parent and no tool column.

## State of a turn (not stored)

`Compaction { used: bool, wrote: bool, failed: bool }` lives for one turn. `failed` makes the one notice the person gets (spec
FR-017); `used` and `wrote` are for the log and the tests.
