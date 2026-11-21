# Research: Chat-Historie verwalten

**Created**: 2026-09-15

## Task intent

Welche bestehenden Domänenmodelle, UI-Komponenten und
Persistenz-/Command-Schnittstellen repräsentieren Chat-Sessions und ihre
Verlaufseinträge, damit die neue Historien-Spec daran anschließen kann?

## Graphify consultation

- Checkout-Klassifikation: Feature-Branch `feature/chat-history-handling`; der
  vollständige Graph-Snapshot vom Fork-Punkt wurde verwendet und nicht gegen
  den Feature-Branch aktualisiert.
- Query: `graphify query "chat history sessions messages conversation sidebar title" --graph /home/haex/Projekte/haex/graphify-out/graph.json --budget 1400`
- Relevante gefundene Knoten: `src-tauri/src/storage/chat_threads.rs`,
  `ChatThread`, `list_threads()`, `update_thread()`, `delete_thread()`,
  `src-tauri/src/chat/thread_commands.rs`, `ThreadPayload`, `list_threads`,
  `list_messages`, `src/composables/useChat.ts`, `Thread`, und
  `src/pages/chat/[instance].vue`.
- Relevante Beziehungen: `ThreadPayload` wird aus `ChatThread` abgeleitet;
  `list_threads` liest die Thread-Metadaten aus `chat_threads`; Nachrichten
  werden separat über `thread_id` geladen. Der Graphify-Output war wegen des
  1.400-Token-Budgets abgeschnitten; die genannten Knoten und Beziehungen
  reichten für die Spec-Navigation aus.

## Source slices reviewed

- `CONTEXT.md`: Vault-Scope, `chat_threads`/`chat_messages` als per-Vault-Daten,
  Sprach- und Internationalisierungsregeln.
- `specs/004-chat-window-handling/spec.md`: neuer Chat-Entwurf,
  Historienauswahl, Nicht-Ziele und bestehende Session-Semantik.
- `src-tauri/src/storage/chat_threads.rs`: Thread-Felder sowie bestehende
  Listen-, Update- und Cleanup-Operationen.
- `src-tauri/src/chat/thread_commands.rs`: öffentliche Thread- und
  Nachrichten-Payloads sowie bestehende CRUD-Grenzen.
- `src/composables/useChat.ts`: Frontend-Thread-Modell und vorhandene
  Listen-/Nachrichtenaufrufe.
- `src/pages/chat/[instance].vue`: aktiver Thread, Historien-Refresh und
  Verhalten beim neuen Chat.
- `src-tauri/src/identity/migrations.rs`: Persistenzschema und
  `created_at`/`updated_at` für Threads sowie `thread_id` für Nachrichten.

## Decisions for the specification

- Die sichtbare Zeit bindet sich an `created_at`, weil die Nutzerfrage den
  Eröffnungszeitpunkt meint und Titeländerungen die zeitliche Einordnung nicht
  verändern sollen. Sichtbar ist die daraus berechnete Dauer in den festen
  kompakten Einheiten `min`, `h` und `d`, zum Beispiel `1min`, `2h` und `5d`.
- `created_at`/`createdAt` wird als Unix-Epoch in Millisekunden behandelt. Die
  Duration-Berechnung clamp't negative oder unbrauchbare Werte auf `0min`,
  damit jeder Verlaufseintrag im vereinbarten Format bleibt.
- Löschen umfasst Thread und zugehörige Nachrichten als eine fachliche
  Aktion; ein verwaister Nachrichtenbestand wäre für den Nutzer nicht
  wiederherstellbar und nicht sichtbar verwaltbar.
- Hover-Aktionen werden auch bei Tastaturfokus verlangt, damit die gewünschte
  Interaktion nicht ausschließlich von einer Maus oder einem Touch-Hover
  abhängt.
- Für die Verlaufszeile wurde die Darstellung als flexible Titelspalte plus
  feste rechtsbündige Zeitspalte gewählt. Bearbeiten und Löschen erscheinen im
  Zwischenraum direkt links neben der Zeit; solange keine Aktion sichtbar ist,
  verbrauchen sie keinen dauerhaften Platz der Titelspalte.
- Bei einer Löschung während eines laufenden Turns wird zuerst abgebrochen und
  der terminale Zustand abgewartet. Erst ein erfolgreicher Abbruch erlaubt die
  anschließende Thread-Löschung; damit bleibt die Löschung bewusst explizit,
  ohne laufende Antwortdaten still zu verwerfen.
