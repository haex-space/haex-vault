# Assistant name and custom wake word (design)

Status: design, not built. Builds on the bundled default wake word (PR #1033) and replaces the
"record four times" idea of spec 066 US7 / research R5.

## Decisions (operator, 2026-11-16)

1. **The name is vault-wide.** One name per vault, the same on every device (not a device row as
   in spec 066's data-model for the custom wake word).
2. **Two forms:** `written` (what is shown and what the LLM writes) and `spoken` (how it is said
   and heard). The default is `{ written: "Haex", spoken: "Hex" }`. Each form is used everywhere
   it belongs: UI texts, chat, system prompt, speech output (`spoken`), wake word (`spoken`).
3. **The harness keeps the spelling.** The LLM and the user always see the written form.
4. **The custom name replaces the default** wake word; "back to default" restores it. The default
   keeps its shipped classifier.
5. **Quality over speed:** a custom name gets a trained classifier handled exactly like
   "Hey Haex" (same embedding, same detector, same sensitivity rules). No always-on speech
   recognition, no recording of four samples.
6. **Training runs in the app on a capable device**, nothing leaves the device. The trained model
   (about 1 MB) is stored in the vault and synced; phones never train, they only run it.
7. **Order:** typed name first (written + spoken), the rest of the pipeline after a spike.

## Data

`assistant.name` (vault preference, synced): `{ written, spoken, aliases[] }`. Valid: 1–2 words,
at most 24 characters, letters, digits and hyphen; `aliases` at most 8, optional spellings the
speech recognition writes for the spoken form.

`assistant.name.model` (vault, binary by hash like the cloned voices): the trained classifier for
one `spoken` form, with its quality report (`recall`, `falseAcceptsPerHour`, `verdict`,
`trainedBy` device, `trainedAt`) and the `spoken` form it was trained for. A training request
stores the `spoken` form too, since training runs on one device while the name syncs to all: a
model is used only while its stored `spoken` form equals the current one. A name without a usable
model keeps the default wake word working and shows "not trained yet".

## Behaviour

- **Wake word:** default name → shipped `hex.onnx`. Custom name with a model → that classifier in
  the same `Detector`. The rule per sensitivity is the same.
- **Speech output:** `tts/spoken.rs` replaces `written` by `spoken` before the voice reads.
- **Harness:** the system prompt gets "The assistant is called X. Always write the name exactly
  like this." A deterministic normalizer (whole words only, case insensitive, outside code, no
  fuzzy matching) keeps the spelling, and only where the name is addressed: in speech recognition
  results `spoken` and `aliases` become `written` (the user said the name); in finished model
  replies only the case of `written` is fixed, `spoken` is left alone since it can be a word of
  its own ("hex encoding").
- **UI texts:** strings that mean the assistant take the name as a placeholder; strings that mean
  the product keep "haex". Audit first.

## Training (the part that needs a spike)

Reuses the recipe of `haex-space/haex-wakeword`: frozen embedding, classifier on 16 × 96, positives
and confusables synthesized, negatives as embeddings, noise and room augmentation, held-out
speakers.

- **Positives:** the in-app voices (Pocket, Qwen) say `spoken` and "Hey" + `spoken`, augmented
  (speed, pitch, noise, room). Piper stays out of the app (GPL-3).
- **Confusables:** generated from a word list by closeness to `spoken` (rhyme, first syllable,
  prefix/suffix), also synthesized. Without espeak-ng the closeness is by letters; the spike
  measures whether that is enough.
- **Negatives:** a shipped or fetched set of speech embeddings, condensed to hard cases (some
  hundred MB instead of 4 GB), plus a few hours of held-out speech for the false-accept estimate.
- **Trainer:** candle (already a dependency), a small network; minutes on a desktop CPU.
- **Quality gate:** target at most 1 false accept in 8 h at "medium" and a recall on held-out
  synthetic speakers. The verdict (good / weak / too weak) is shown before the name is used.
  A one-syllable name is expected to come out weak (the lone "Hex" reaches about 50 %); the page
  says so in plain words.
- **Who trains:** a device that meets a size/RAM bar picks up a request row in the vault
  (`requested → training → ready | failed`). A vault with only phones keeps the default and says
  why.

## Steps

1. **Name as data** (PR 1): `assistant.name`, settings fields, placeholders in the UI, system prompt
   sentence, normalizer, speech output. Makes the default name consistent and is useful alone.
2. **Spike** (research, no app code): with the `haex-wakeword` pipeline, train 10 German and
   10 English names from Pocket-only positives and letter-based confusables; compare recall and
   false accepts with the Piper + Pocket recipe. Decides whether in-app voices are enough and how
   large the negative set must be. Needs about 5 GB of downloads and GPU hours, so it is run only
   with the operator's go-ahead.
3. **Trainer, quality gate, vault model, sync** (PR 2+): only if the spike passes; otherwise the
   fallback is a training service (operator's earlier option) or lower expectations per name.

## Not in scope

Renaming the product, the extension/marketplace wording, the recording of samples (US7 as
written), a training service.

## Open

- The size/RAM bar for "capable device".
- Where the condensed negative set lives (in the binary, or fetched like the wake word update).
