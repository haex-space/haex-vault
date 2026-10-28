# Spoken secrets go from the password service straight to a local voice

Status: proposed
Date: 2026-10-28

## Context

Spec 066 (voice assistant) lets the user ask for a secret by voice ("what is the guest Wi-Fi
password?") and have haex read it aloud after a spoken confirmation, at every approval level
(clarification 2026-10-28). ADR-0007 says the built-in agent never receives a secret, and specs
034 and 021 repeat that. The voice conversation runs through the built-in agent (specs 003/032), so
the agent cannot be the one that reads the secret.

## Decision

- A native tool `assistant.secret.speak { itemId, field }` is offered to the built-in agent **only
  in voice conversations**. Its result to the model is `{ spoken: true }` or a reason why not; it
  never carries the secret.
- Before every retrieval haex asks aloud, names entry and field, and waits for an unambiguous
  spoken "yes". Silence, refusal or an unclear answer read nothing. This holds at every approval
  level, including Auto.
- Only `assistant::approval` can build the `SpokenConsent` value that this "yes" produces. The
  password service gets a new entrance `reveal_for_speech(item, field, consent)`. It counts as a
  read by the **user** (like "Show" in the window) and needs the consent value. There is still no
  path where `Caller::BuiltinAgent` reaches a secret.
- The secret goes from the service directly into the spelling and synthesis code of a **local**
  voice. It never reaches the model input, an event, a log line, `Debug` output, the transcript
  (`chat_messages` gets a note "password of ‚X‘ read aloud") or an external speech service. If no
  local voice is installed, haex shows the secret masked on screen instead.
- To allow "say it again", the value stays in memory (`Zeroizing`) until the follow-up window ends,
  then it is overwritten.

## Consequences

- ADR-0007 stays valid unchanged: the agent decides _that_ something is read, the user decides
  _whether_, and only the backend ever holds the value.
- Anyone in the room who says the wake word and "yes" can get a secret read aloud. The operator
  chose this ("everything by voice"); haex points it out once when the mode is first enabled
  (spec 066 FR-037). Binding confirmation to the user's voice is left to a later spec.
- A contract test (`assistant_secret_contract.rs`) checks every outward path (events, log, model
  input, stored messages, external requests) for the secret.

## Alternatives

- **Give the secret to the model and let it read the answer** — breaks ADR-0007 and leaks the
  secret to cloud providers.
- **Let the confirmation follow the approval level** — Auto would read secrets without asking;
  rejected, in line with spec 021 (secrets always ask).
- **Use the external speech service as well** — would send the secret off the device.
