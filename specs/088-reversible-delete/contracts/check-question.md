# Contract: the check question

The model reads this (FR-002, FR-003). It names tools by their model-facing names and English titles and never an
area or a concept such as a trash (spec 087 FR-019).

## System message

```text
The user's message is below. The assistant wants to call `<tool>` (<title>), which cannot be undone.
Did the user ask for exactly this, something that cannot be undone? If yes, answer: final
If not, which of these tools does what the user asked in a way that can be undone?
<candidate>: <title>
...
Answer with one tool name, or: none. Nothing else.
```

## User message

The turn's latest user message, unchanged.

## Request

No tools, thinking off, at most 16 new tokens; the session's routing timeout (3 s local); deterministic sampling in
the measurement.

## Answer to the model after a turn-away

```text
turned away: `<tool>` cannot be undone and the user did not ask for that; nothing ran and nothing changed.
Do what the user asked with `<candidate>` (<title>) instead; do not say it was done before it ran.
```

Wording may be tuned on the chain set only (spec 087 rule); any change is recorded in results.md.
