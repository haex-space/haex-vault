# ADR 0003 — Explicit Sync Policy (Doppel-Buchführung für `SPACE_SCOPED_CRDT_TABLES`)

- **Status:** Accepted
- **Date:** 2026-08-18
- **Deciders:** Martin Drechsel

Spezifiziert die Enforcement-Mechanik für die Grenze zwischen owner-privatem CRDT-Sync und
Shared-Space-Sync (Lücke aus `docs/security/invariants.md` §I8 Sync Safety). Die konkreten
Tests liegen in `src-tauri/src/crdt/scanner_tests.rs` (Phase 3b); Impl-Details in
`docs/plans/2026-08-18-security-hardening-phase3b-implementation.md`.

---

## Kontext

Sync-Eligibilität in haex-vault hat heute zwei Schichten:

1. **Layer 1 — CRDT-Injection** (`CrdtTransformer` in
   [src-tauri/src/crdt/transformer/mod.rs:109-118](../../src-tauri/src/crdt/transformer/mod.rs#L109-L118)):
   Migrations-DDL bekommt implizit die drei CRDT-Metadaten-Spalten
   (`haex_hlc`, `haex_column_hlcs`, `haex_column_sigs`). Opt-out ist der
   Namens-Suffix `_no_sync` (`is_crdt_sync_table`). Layer-1-Injection macht eine Tabelle
   für **owner-persönlichen Sync** (Sync-Server, andere Devices desselben Owners) eligible,
   nicht für Shared-Space-Sync.

2. **Layer 2 — Shared-Space-Whitelist** (`SPACE_SCOPED_CRDT_TABLES` in
   [src-tauri/src/crdt/scanner.rs:37-55](../../src-tauri/src/crdt/scanner.rs#L37-L55),
   aktuell 8 Einträge inkl. Register/Delete-Log/Compaction-Anchor):
   opt-in Whitelist. Nur Tabellen auf dieser Liste dürfen einen Space-Delivery-Stream kreuzen.
   Alles andere CRDT-getrackte ist owner-personal-sync-only.

`SPACE_SCOPED_CRDT_TABLES` wird zusätzlich in der Invariante `I12` (`invariants.md`) als
Default-Deny-Whitelist referenziert und ist der Enforcement-Anker für `I8`.

### Beobachtete Lücke

Der originale Security-Review (§8.4/§8.5) und der Phase-3-Pickup-Prompt
(`docs/plans/2026-08-18-security-hardening-phase3-pickup.md`) framten das Problem als:
"Sync-Eligibilität wird durch Namens-Konvention entschieden, die kein Compiler prüft — ein
Developer, der eine neue Tabelle anlegt und `_no_sync` vergisst, aktiviert Sync unbemerkt."

Beim Design-Spike wurde dieses Framing als überzogen verworfen (siehe Alternativen unten). Der
verbleibende, konkrete Fehlerpfad ist **Layer-2-spezifisch**: eine Änderung an
`SPACE_SCOPED_CRDT_TABLES` fügt versehentlich eine vault-private `haex_*`-Tabelle zur
Whitelist hinzu (z. B. beim Reviewen einer PR, weil der Tabellen-Name plausibel wirkt). Konsequenz
wäre ein Bruch der Invarianten `I8` (Sync Safety) und `I12`
(Vault-Scoped-Table Confinement) — vault-privater Inhalt fließt an dritte Space-Member.

### Threat-Model der beteiligten Rollen

Wer welche Tabellen anlegen und in welche Liste eintragen kann, ist die relevante Trust-Grenze:

- **`haex_*` Tabellen** dürfen ausschließlich Core-Devs von haex-vault via
  Rust-Migrations anlegen. Extensions bekommen den Prefix `{public_key}__{name}__`
  (`is_auto_allowed_table`, siehe `invariants.md` §I3) und können `haex_*` weder anlegen noch
  schreibend erreichen. Der Fehler-Pfad "Extension-Autor kompromittiert Sync-Grenze durch neue
  Tabelle" existiert nicht.
- **`SPACE_SCOPED_CRDT_TABLES`** liegt in `crdt/scanner.rs`, ebenfalls ausschließlich
  Core-Dev-Editierbar. Extensions haben keinen Einfluss.

Damit ist der einzige realistische Angriffs- bzw. Fehler-Pfad ein Core-Dev-Versehen. Die
Barriere gegen dieses Versehen ist heute: Reviewer-Aufmerksamkeit + Konventions-Restwissen.
Kein CI-Test failt.

---

## Verworfene Alternativen

**Alternative A — Default flippen, Policy-Registry mit `Local | Private | Shared` Enum.**
Der Handoff-Plan schlug vor, `CrdtTransformer` per Marker (SQL-Kommentar `-- @crdt: shared` oder
Rust-Enum-Eintrag pro Tabelle) explizit zu opt-in-en. Verworfen weil:

1. Die einfache Regel "jede Tabelle wird gesynct außer sie hat `_no_sync`-Suffix" ist Teil des
   bewussten Design-Vertrags gegenüber Extension-Autoren. Ein Enum-Registry würde jede
   Extension-Migration verkomplizieren, ohne einen Angriffs-Pfad zu schließen, den Extension-Autoren
   überhaupt betreten können.
2. Für `haex_*`-Tabellen bleibt der einzige Angriffs-Pfad Core-Dev-Versehen bei
   `SPACE_SCOPED_CRDT_TABLES`. Ein Enum-Registry über alle 35 CRDT-getrackten `haex_*`-Tabellen
   ist unverhältnismäßiger Impact für diese eine Bedrohung.
3. Migration von 35 Tabellen wäre breaking (auch wenn keine Prod-Nutzer nach
   `[[no-backward-compatibility-yet]]`), und schafft eine zweite Konvention, die parallel zur
   `_no_sync`-Regel gepflegt werden müsste.

**Alternative B — Build-time / test-time Lint über CRDT-Injection (haex_* ohne Registry-Eintrag
= fail).** Verworfen aus denselben Gründen wie A: die Bedrohung ist nicht am Layer 1 lokalisiert,
und die Coverage-Kosten stehen in keinem Verhältnis zum Gewinn.

**Alternative C — Doku-only, kein CI-Enforcement.** Verworfen weil Reviewer-Aufmerksamkeit
bereits heute die einzige Barriere ist. Das Ziel dieses ADRs ist gerade, diese Aufmerksamkeit
strukturell zu erzwingen.

---

## Entscheidung

1. **Layer 1 bleibt exakt wie er ist.** Der `_no_sync`-Suffix ist die kanonische, einheitliche
   Opt-out-Regel für Core-Devs und Extension-Autoren. `CrdtTransformer::is_crdt_sync_table()`
   wird nicht angefasst. Keine Umbenennung der 15+ bestehenden `*_no_sync`-Tabellen.

2. **`SPACE_SCOPED_CRDT_TABLES` bleibt hartcodierte Liste in
   [crdt/scanner.rs](../../src-tauri/src/crdt/scanner.rs#L37-L55).** Kein Policy-Registry, keine
   Enum-Kategorien, keine Migration in eine externe Konfig-Datei. Rust bleibt Source of Truth per
   [CONTEXT.md](../../CONTEXT.md) "Shared-Space-Whitelist"-Eintrag.

3. **Neu: Doppel-Buchführungs-Snapshot-Tests in
   `src-tauri/src/crdt/scanner_tests.rs`.** Der Enforcement-Mechanismus. Vier Tests, jeder
   mit dem Funktions-Namen unter dem er läuft:

   - **Test 1 — `SPACE_SCOPED_CRDT_TABLES`-Snapshot**
     (`space_scoped_crdt_tables_matches_documented_expectation`). Eine im Test separat
     notierte `EXPECTED`-Liste (kein `use` aus scanner.rs — vollständige, wörtliche
     Duplikation) mit Begründungs-Kommentar pro Eintrag. Der Test vergleicht die
     Runtime-Konstante gegen `EXPECTED` und failt bei Diff mit einer Message, die die drei
     nötigen Schritte nennt: `EXPECTED` anpassen, Begründungs-Kommentar pro Eintrag
     ergänzen, ADR referenzieren.

   - **Test 2 — `MEMBERSHIP_SYSTEM_TABLES`-Snapshot**
     (`membership_system_tables_matches_documented_expectation`). Analoges Setup. Diese
     Subset-Liste hat eigene Sicherheits-Konsequenzen (read-only Member push-Rechte, siehe
     `crdt/scanner.rs:73-78`) und braucht die eigene Buchführung.

   - **Test 3 (a) — Konstanten-Präsenz-Check über `table_names::TABLE_*`**
     (`whitelisted_tables_exist_as_generated_constants`). Für jeden Eintrag der beiden
     Whitelists MUSS eine kanonische Konstante aus dem generierten `table_names`-Modul
     (`src-tauri/generator/table_names.rs` → `crate::table_names::TABLE_*`) mit exakt
     diesem String-Wert existieren. Fängt Rename/Drift zwischen Snapshot und
     Schema-Generator.

   - **Test 3 (b) — Schema-Präsenz-Check gegen das Migrations-Schema**
     (`whitelisted_tables_exist_in_the_migration_schema`). (a) allein genügt nicht: die
     `TABLE_*`-Konstanten werden aus der **handgepflegten** Registry
     `src/database/tableNames.json` generiert, nicht aus den Migrationen. Beide Quellen
     können auseinanderlaufen — eine Migration darf eine Tabelle droppen, während der
     JSON-Eintrag stehen bleibt, und genau das ist der "still-defekte Sync-Path". Der Test
     spielt daher alle Drizzle-Migrationen in Journal-Reihenfolge auf eine In-Memory-DB und
     prüft pro Whitelist-Eintrag `get_table_schema`. Die manuellen Migrationen
     (`database/migrations-manual`) bleiben ausgeklammert: sie enthalten nur Trigger und
     referenzieren die CRDT-Meta-Spalten, die der Produktions-Runner via `CrdtTransformer`
     injiziert.

   Der Kern-Effekt: jede beabsichtigte Änderung an einer Sicherheits-Whitelist erfordert eine
   bewusste, Review-sichtbare Änderung an einer **zweiten** Stelle, an der jeder Eintrag mit
   Begründung dokumentiert ist. Analog zum Delete-Propagation-Register-Gate (I11) und der
   Erfahrung aus `[[verify-review-findings-dont-blind-comply]]`.

---

## Konsequenzen

### Positiv

- Der real existierende Fehler-Pfad (Core-Dev fügt versehentlich `haex_*`-Tabelle zu
  `SPACE_SCOPED_CRDT_TABLES` hinzu → I8/I12-Bruch) wird von einem `cargo test crdt::`-Lauf
  gefangen. Die Barriere ist explizit, nicht mehr Reviewer-Restwissen.
- Doppel-Buchführung zwingt bei intendierter Änderung zu einem strukturierten Sicherheits-Kommentar
  im Test, der in jeder PR-Diff sichtbar ist.
- Minimaler Code-Impact: nur ein neues Test-Modul, keine Änderung an `CrdtTransformer`,
  `scanner.rs`, Migrations oder Wire-Format.
- Rust bleibt Source of Truth (CONTEXT.md-Vertrag).
- Keine Ecosystem-Auswirkung auf Extension-Autoren (die `_no_sync`-Regel ist ihr einziger
  Vertrag und bleibt unverändert).

### Neutral

- Snapshot-Tests haben Wartungs-Overhead bei jeder intendierten Whitelist-Änderung. Das ist
  der Zweck, nicht ein Nachteil.
- Der Test dupliziert die Liste. Das ist absichtlich (single-source-of-truth wäre der
  Angriffs-Pfad selbst).

### Negativ

- Layer 1 (`_no_sync`-Konvention) bleibt eine "stumme" Konvention ohne CI-Enforcement.
  Bewusste Akzeptanz nach Threat-Model oben: `haex_*` können nur Core-Devs anlegen, Extension-Devs
  bekommen `_no_sync` als einheitliche einfache Regel. Wenn dieser Trust jemals gebrochen wird
  (z. B. wenn Extensions selbst `haex_*`-Tabellen anlegen dürften), müsste dieser ADR revidiert
  werden.
- Doppel-Buchführung ist keine strukturelle Sicherheit — sie ist eine erzwungene Prozess-Hürde.
  Ein Angreifer mit Commit-Rechten könnte Whitelist und Snapshot in derselben PR ändern. Das ist
  akzeptierter Trust-Model-Rahmen: Core-Dev-Reviewer-Aufmerksamkeit + strukturierter Kommentar-Block
  bleiben die letzte Barriere.

---

## Nicht Bestandteil dieser ADR (Follow-up-Kandidaten)

- Layer-1-Enforcement für `haex_*` (Test, der neue `haex_*`-Tabellen zwingt, entweder `_no_sync` zu
  haben oder in einer zusätzlichen "explicit sync"-Liste zu stehen). Kein Handlungsbedarf nach
  aktuellem Threat-Model, wird bei einer Trust-Boundary-Änderung revidiert.
- Migration `SPACE_SCOPED_CRDT_TABLES` in externe JSON/TOML-Konfig. Bewusst nein.
- Runtime `sync_mode`-Spalte pro Tabelle. Compile-time Enforcement reicht.
- Erweiterung des Snapshot-Tests auf `is_register_target_forbidden()`-Logik. Kandidat für eine
  Folge-ADR, wenn das Share-Register generischer wird.
- Kollision-Check mit dem "Single Authority Plane"-Plan
  (`docs/plans/2026-08-14-single-authority-space-model.md`) — dessen Enforcement liegt auf einer
  anderen Ebene (Authorization) und tangiert dieses ADR nicht direkt.

---

## Referenzen

- [docs/adr/0002-shared-space-authenticity-and-confidentiality.md](0002-shared-space-authenticity-and-confidentiality.md)
  §5 (Invariante I1), §7 Phase-3.a.
- [docs/security/invariants.md](../security/invariants.md) §I8 (Sync Safety), §I12
  (Vault-Scoped-Table Confinement) — beide werden per Verweis auf dieses ADR aktualisiert.
- [src-tauri/src/crdt/scanner.rs:37-55](../../src-tauri/src/crdt/scanner.rs#L37-L55) —
  `SPACE_SCOPED_CRDT_TABLES`.
- [src-tauri/src/crdt/scanner.rs:73-78](../../src-tauri/src/crdt/scanner.rs#L73-L78) —
  `MEMBERSHIP_SYSTEM_TABLES`.
- [src-tauri/src/crdt/transformer/mod.rs:109-118](../../src-tauri/src/crdt/transformer/mod.rs#L109-L118)
  — `is_crdt_sync_table()` mit `_no_sync`-Suffix.
- [docs/plans/2026-08-18-security-hardening-plan.md](../plans/2026-08-18-security-hardening-plan.md)
  Phase 3 — Impl-Plan liegt in Phase-3b-Datei.
- [docs/plans/2026-08-18-security-hardening-phase3-pickup.md](../plans/2026-08-18-security-hardening-phase3-pickup.md)
  — dieser Design-Spike (Handoff, jetzt umgesetzt).
