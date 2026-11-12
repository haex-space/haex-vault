# Contract: the check question

The model reads this (FR-002, FR-003). It names tools by their model-facing names and English titles and never an
area or a concept such as a trash (spec 087 FR-019).

## System message

```text
The assistant wants to call `<tool>` (<title>). It cannot be undone, so it is only right when the user explicitly says permanently, for good or never to be restored. Saying only what to do is not enough.
If the user's message below says so explicitly, answer: final
Otherwise answer with the tool from this list that does what the user asked and can be undone:
<candidate>: <title>
...
Never answer `<tool>`. Answer with one word: final, or one tool name from the list.
```

## Messages

The conversation's user messages, unchanged, in order; before the latest one, the assistant's last text answer as an
assistant message when it ends with a question. Tool calls and results are left out.

## Request

No tools, thinking off, at most 16 new tokens; the session's routing timeout (3 s local); deterministic sampling in
the measurement.

## Answer to the model after a turn-away

```text
turned away: `<tool>` cannot be undone and the user did not ask for that; nothing ran and nothing changed.
Call `<candidate>` (<title>) now to do what the user asked; it needs no confirmation, so do not ask the user first,
and do not say it was done before it ran.
```

Wording may be tuned on the chain set only (spec 087 rule); any change is recorded in results.md. This is candidate
c6 of T018 with answer a2; c1 and a1 (the first wordings) are in results.md.
