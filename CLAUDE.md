# CLAUDE.md - Projektanweisungen

## Tech-Stack & Toolchain

- **Package-Manager: `pnpm`** — `pnpm-lock.yaml` ist das Signal. NIE `npm install`.
- **Keine Google-Services** — kein Firebase/FCM/Maps/AdMob. Play App Signing ist der einzige Touchpoint.
- **Keine Backward-Kompatibilität nötig** — noch keine Prod-Nutzer. Keine Legacy-Shims/Backfills vorschlagen; breaking Änderungen sind erwünscht.

## Branching & PR-Workflow

**Feature/fix work → PR gegen `develop`, niemals direkt gegen `main`.**

- `develop` ist der Entwicklungs-Branch. Alle `feat/*`, `fix/*`, `chore/*`, `refactor/*` PRs gehen hier rein.
- `main` ist der Release-Branch. Updates ausschließlich über:
  - `release-please--*` (automatisierte Release-PRs)
  - `sync-*` (Sync-Helfer wie `sync-cargo-lock`, `sync-develop/*`)
  - `develop` → `main` (Release-Merge, von release-please orchestriert)
  - `hotfix/*` (echte Production-Hotfixes; muss zusätzlich nach develop gemergt werden)
- Die Policy ist via `.github/workflows/enforce-main-base.yml` hard-gated — PRs mit unzulässigem Head-Branch failen CI.
- Beim Anlegen eines PRs immer explizit `--base develop` setzen (oder `gh pr edit --base develop` falls aus Versehen `main` gewählt).
- **`develop` → `main` NIE squash-mergen** — release-please parst nur Commit-Subjects, ein Squash killt die Release-Notes-Ableitung.
- **`gh pr view <N>` vor jedem Push** auf einen alten Branch — verhindert Push auf einen bereits gemergten/geschlossenen PR.
- **Bei `gh`-Permission-Fehler sofort `gh auth switch --user haex-space`** und retry, nicht diagnostizieren.
- **Companion-PR Merge-Order: Vault ZUERST**, dann `haex-e2e-tests`. Sonst geht e2e-tests `main` rot.
- **Stacked-PR Gotcha**: Wird die Base-PR gemergt bevor die stacked-PR gemergt ist, muss die stacked-PR auf `develop` re-targeted werden.
- **Dependabot-PRs, die Android-E2E failen**: `gh pr close` + `gh pr reopen`, NICHT `rerun --failed` (Secrets-Scope).

## CRDT & Datenbank-Konventionen

- **Jede SQL auf `haex_*`-Tabellen MUSS durch `execute_with_crdt` / `select_with_crdt`** laufen. Chokepoint für Column-Signing und HLC-Bookkeeping.
- **Primary Keys sind UUIDs und werden NIE geändert** — gilt auch für Fix-Sketches in Plänen.
- **Löschungen: `haex_deleted_rows` (Delete-Log)**, keine `haex_tombstone`-Spalte.
- **Migrations: `--> statement-breakpoint` ist Pflicht** zwischen Statements. Sonst silent multi-statement failure.
- **`SPACE_SCOPED_CRDT_TABLES` ist hartcodiert** (ADR 0003) — Sync-Whitelist mit Doppel-Buchführungs-Tests.

## Test- & CI-Konventionen

- **Rust-Tests in separaten `tests.rs`-Files** pro Modul, gemounted via `mod tests;`.
- **`cargo fmt` vor jedem Push** — enforced seit PR #469, sonst rotes CI.
- **Keine literalen Seeds/Nonces in Tests** — `rand::random()` verwenden, sonst flagged CodeQL als hardcoded credential.
- **e2e-Test-Rig lokal**: `cargo tauri build --no-bundle` + `docker cp` ins container-basiertes Rig; `CI=true` Pflicht.
- **`haex-e2e-tests` spricht KEIN QUIC direkt** — kein Wire-Harness; nur observable Properties über die App-Oberfläche.
- **Docker-E2E ist host-load-sensitiv**: vor jeder Bug-Diagnose zuerst `uptime`/`free -h` — createLocalSpaceViaUI racet unter Last.
- **macOS/Windows E2E: bei rotem Native-Leg NIE `rerun --failed`** — kann Backend-Queue-Starvation triggern. Neu pushen.
- **Security/Sync/Auth/CRDT-Features brauchen e2e-Attack-Specs** im `haex-e2e-tests`-Repo, nicht nur Unit-Tests.

## Rust-Patterns

- **Permission-Check Helpers folgen der Convention `<resource>_matching_status` + `deny_first_precedence`** — kein `iter().find()`.
- **File-Split Playbook** (>800 LoC → Verzeichnis): siehe `.claude/patterns.md`. Verzeichnis statt Datei, `pub use` für die alte Oberfläche, STAGING HYGIENE (kein `git add -A`).

## Sync- & P2P-Prinzipien

- **`haex_logs` ist Owner-only** — geht nie via CRDT-Sync zu anderen Peers.
- **DID-Auswahl beim Space-Beitritt ist immer explizit** — niemals auto-default auf eine bestehende DID.
- **Targeted-Invite-Zustellung ist P2P-liveness-abhängig** — kein Server-Fallback. E2e testet passiven Empfänger nicht.
- **Extension-Command-Surface = Capability-Allowlist** (`permissions/extension-commands.toml`) — der Chokepoint, nicht per-call Checks.

## Codebase-Fragen & Recherche

- **Bei Codebase-Fragen zuerst `graphify query "<frage>"`** wenn `graphify-out/graph.json` existiert. Auch `graphify path`/`graphify explain`. Deutlich billiger als grep + Read.
- **`graphify-out/wiki/index.md` für breite Navigation** statt raw source browsing.
- **`graphify-out/GRAPH_REPORT.md` nur für Architektur-Reviews** oder wenn query/path/explain nicht reichen.
- **Nach Code-Änderungen: `graphify update .`** (AST-only, no API cost).
- **`claude -p` für graphify-Subsessions braucht Permission-Flags**, sonst hängt Sub-Session auf Bash-Approval.

## Zusammenarbeit & Kommunikation

- **Design-Iterationen ohne Modal-Questions** — Optionen inline anbieten statt `AskUserQuestion` zu spammen.
- **Keine unbekannten Akronyme in User-Text** — in Doku/PR-Body ausschreiben.
- **Stated Stack folgen, nicht Nachbar-Datei kopieren** — wenn ein Stack genannt wurde, gilt der, auch wenn die adjacent Files ein anderes Muster haben.
- **Code-Review-Findings nicht blind übernehmen** — Call-Sites verifizieren, Autoritätsmodell kann abweichen. CodeRabbit ist oft plausibel-aber-falsch.

## Knowledge Database (`.claude/`)

Für dieses Projekt gepflegte Themen-Docs, keine Chronik:

- `overview.md`, `architecture.md`, `patterns.md`, `decisions.md` — stabile Referenz
- `problems.md` — bekannte offene Probleme mit Repro & Status
- `federation.md`, `shared-spaces.md`, `iroh-research.md`, `openmls-research.md`, `mls-gaps.md`, `e2e-testing.md`, `remote-storage.md`, `database.md`
- Aktive Pläne: `plan-*.md`

**Update-Regeln**:
- Sofort bei Architektur-Entscheidungen, neuen Erkenntnissen, gelösten Problemen
- Prägnant, Fakten statt Prosa
- Veraltetes löschen, nicht archivieren
- Kein Session-Log

## Git Commits

- Keine Claude-Referenzen ("Generated with Claude Code", "Co-Authored-By: Claude") in Commit-Messages.
- Commit-Messages auf Englisch, kurz und prägnant.
