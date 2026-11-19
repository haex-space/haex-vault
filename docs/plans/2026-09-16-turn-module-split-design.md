# `chat/turn.rs` → `chat/turn/` Modulsplit

Status: akzeptiert (2026-09-16)

## Ausgangslage

`src-tauri/src/chat/turn.rs` hatte 1116 LoC, davon `run_turn` allein ~580.
Der Datei-Header trug eine dokumentierte spaex-500-LoC-Ausnahme mit
Split-Plan — formal konform, aber der Plan war fällig.

Konkrete Befunde:

- Der Block "Persistenz fehlgeschlagen → `chat-message-error` →
  `chat-turn-complete{Error}` → return" stand **5× wortgleich** drin
  (je ~22 Zeilen).
- ~20 Emit-Call-Sites wiederholten
  `serde_json::to_value(X).expect("X always serializes")`.
- Verschachtelung in `run_turn` erreichte 5–6 Ebenen:
  `loop` → `if !tool_calls.is_empty()` → `for` (Planung) →
  `join_all(async move)` (Ausführung) → `for` (Persistenz).

Der Header nannte als Split-Hindernis, dass die Match-Arme
`parent_id` / `rounds_used` / `next_tool_created_at` über Iterationen
hinweg teilen. Genau das lösen Struct-Felder auf.

## Zielform

```
src-tauri/src/chat/turn/
├── mod.rs           run_turn (Wrapper), TurnRunner-Definition, Loop, Re-Exports
├── persist.rs       persist_message/_final/_cancelled, empty_tool_message,
│                    emit_event, fail, now_ms
├── persist_tests.rs
├── step.rs          StepOutcome, StepResult, RetryDecision, retry_backoff,
│                    retry_or_bail, StreamStartError, start_step_stream, run_step
├── step_tests.rs
└── tool_round.rs    ToolPlan, RoundOutcome, read_permission_mode,
                     plan_calls / execute_plans / persist_round /
                     append_round_to_request / end_tool_limit / end_cancelled
```

`TurnRunner` wird in `mod.rs` definiert, die inhärenten `impl`-Blöcke liegen
in den Submodulen; Felder sind `pub(super)`.

```rust
struct TurnRunner<'a> {
    db: &'a haex_crdt::Database,
    chat_state: &'a ChatState,
    session: &'a ActiveSession,
    thread_id: Uuid,
    assistant_message_id: Uuid,
    cancel: CancellationToken,
    emit: &'a mut (dyn FnMut(&'static str, Value) + Send),
    request: ChatRequest,   // wächst über die Runden
    parent_id: Uuid,        // Loop-State
    rounds_used: usize,     // Loop-State
    next_created_at: i64,   // Loop-State
}
```

## Kontrollfluss

```rust
async fn run(mut self, stream: AdapterStream, initial_attempt: usize) {
    let mut next_stream = Some(stream);
    let mut attempt = initial_attempt;
    loop {
        let step = self.run_step(next_stream.take(), &mut attempt).await;
        if step.wants_tools() {
            match self.tool_round(step).await {
                RoundOutcome::Continue => continue,
                RoundOutcome::Ended => return,   // Events bereits emittiert
            }
        }
        self.finish_turn(step).await;
        return;
    }
}

async fn tool_round(&mut self, step: StepResult) -> RoundOutcome {
    if self.persist_interim_text(step.assembled).await.is_err() { return Ended; }
    if self.cancel.is_cancelled() { self.end_cancelled().await; return Ended; }

    let plans = self.plan_calls(step.tool_calls).await;  // sequentiell, emittiert Ask
    let executed = self.execute_plans(plans).await;      // join_all, nebenläufig

    if self.cancel.is_cancelled() { self.end_cancelled().await; return Ended; }
    if self.persist_round(&executed).await.is_err() { return Ended; }
    self.append_round_to_request(&executed);

    self.rounds_used += 1;
    if self.rounds_used >= MAX_TOOL_ROUNDS { self.end_tool_limit().await; return Ended; }
    Continue
}
```

Das bisherige 7-Tupel-Destructuring des `StepOutcome` wird ein benanntes
`StepResult` mit `fn wants_tools(&self) -> bool`.

Maximale Verschachtelung danach: 3 Ebenen.

## Stabile Außenfläche

Kein Call-Site außerhalb von `chat/turn/` wird angefasst. Über Re-Exports
in `turn/mod.rs` bleiben gültig:

- `super::turn::{now_ms, run_turn, start_step_stream, StreamStartError}`
  (aus `chat/commands.rs`)
- `haex_lib::chat::turn::{run_turn, MAX_TOOL_ROUNDS, MAX_RETRY_ATTEMPTS}`
  (aus `tests/chat_tool_loop_*.rs`, `tests/common/tool_loop_fixture.rs`)

## Nicht-Ziele

Keine Verhaltensänderung. Unverändert bleiben:

- Reihenfolge und Nutzlast jedes emittierten Events
- `created_at`-Tie-Breaking (`next_created_at.max(now_ms()).saturating_add(1)`)
- die beiden Cancel-Checkpoints innerhalb einer Tool-Runde
- `read_permission_mode` pro Tool-Call, nicht pro Runde (T024B)
- sequentielles Minten der Approval-Oneshots vor jeder Nebenläufigkeit (T024A)

## Verifikation

`cargo test --manifest-path src-tauri/Cargo.toml` (beide Feature-Beine der
CI-Matrix) muss vor und nach dem Umbau identisch grün sein. Sicherheitsnetz
sind ~1900 Zeilen Integrationstests: `tests/chat_tool_loop_core.rs`,
`chat_tool_loop_retry.rs`, `chat_tool_loop_permissions.rs` und
`tests/common/tool_loop_fixture.rs`.

Die 500-LoC-Ausnahme im Header-Doc entfällt ersatzlos.
