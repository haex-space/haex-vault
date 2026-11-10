# Contract: one GPU memory ledger for chat and speech

Owner: the llama.cpp engine (`src-tauri/src/llm/llamacpp/memory.rs`). Spec 080 (kept context) and 079 (drafter)
change the same planner; this contract fixes what spec 081 needs from it, so those changes can leave room for it.
Implemented in delivery step 4, after 080's planner change is on `main`.

## Occupants

```rust
pub enum Occupant {
    /// The loaded chat model with everything its plan counted (layers on the GPU, kept context, drafter).
    Chat,
    /// The loaded speech model (Qwen3-TTS backbone, audio part, context and compute buffers).
    Speech,
}
```

One instance of each at most: haex keeps one chat model and one speech model loaded (spec 070, 066).

## API

```rust
/// Process-wide; created on first use like `model::backend()`.
pub fn ledger() -> &'static Ledger;

impl Ledger {
    /// Records what `occupant` placed on the GPU after its load; 0 when it runs on the CPU.
    pub fn placed(&self, occupant: Occupant, device_bytes: u64);

    /// Forgets `occupant` after its unload.
    pub fn released(&self, occupant: Occupant);

    /// The speech model asks before loading: GPU only if the device's free memory minus `need`
    /// still holds the last chat plan's full-offload need (or the device margin when no chat
    /// model was loaded since start). Never evicts anything.
    pub fn place_speech(&self, need: u64, free: FreeMemory) -> SpeechPlacement; // Gpu | Cpu

    /// The chat load asks before planning: if the speech model holds GPU memory and the chat
    /// plan with that memory added would put more layers on the GPU, the speech model is told
    /// to move to the CPU, and this waits (bounded, 5 s) until it has released.
    pub async fn make_room_for_chat(&self, shape: &ModelShape, free: FreeMemory) -> FreeMemory;

    /// The speech side registers how to move itself to the CPU.
    pub fn on_evict_speech(&self, evict: Box<dyn Fn() + Send + Sync>);
}
```

`FreeMemory` and `ModelShape` are the planner's existing types. The ledger stores the last chat plan's full-offload
need (from `ModelShape`) so `place_speech` can keep room for a chat model that is unloaded between answers.

## Rules

1. **Chat wins** (spec 081 FR-009): `make_room_for_chat` evicts the speech model whenever that gives the chat
   model more GPU layers or a larger context. Speech never evicts chat.
2. **No abort** (FR-009a): both sides plan against llama.cpp's reported free memory at that moment; the ledger
   only subtracts what it knows is reserved. A speech load that the ledger sends to the GPU and that still fails
   to allocate retries once on the CPU.
3. **Between parts**: the speech side honours an eviction at the next part boundary; a part that is generating
   finishes first (at most about 1 s on the GPU). `make_room_for_chat` waits for `released(Speech)`.
4. **No GPU, no ledger work**: on CPU-only builds and devices all calls are cheap no-ops that answer `Cpu`.
5. **Kept context and drafter** (080, 079): their bytes are part of `Chat`'s `device_bytes`; the ledger does not
   split them.

## Tests

`memory_tests.rs` (or a new `ledger_tests.rs`): speech goes to the GPU with room for the last chat plan; speech
stays on the CPU when it would push the chat model off; a chat load evicts speech when that adds GPU layers and
does not when it would not; an eviction during a generating part completes after the part; CPU-only answers `Cpu`.
