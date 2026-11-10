# Quickstart: Sync-Regeln von Hand prüfen

Voraussetzung: zwei lokale Ordner (zum Beispiel `<Tmp>/a` und `<Tmp>/b`), ein S3- oder WebDAV-Speicher
(Spec 038/050) und, für den Gerätefall, ein zweites eigenes Gerät mit eingeschaltetem „Geräte-Dateizugriff“
(Spec 056).

Automatische Prüfungen vor der Handprüfung (gezielt, der Rechner hat 31 GB und bricht bei breiten Builds ab):

```bash
cargo test -j 4 --lib file_sync::
cargo test -j 4 --lib identity::migrations_sync_rules
pnpm check:files && pnpm check:settings && pnpm check:agent-actions
pnpm typecheck && pnpm typecheck:scripts && pnpm lint && pnpm format:check
cargo clippy -j 4 --lib -- -D warnings
```

Rust-Zweige mit `cfg` zusätzlich für `aarch64-linux-android` linten.

## 1. Einseitig sichern (Story 1)

1. Dateibrowser → „Sync-Regeln“ → „Neue Regel“. Quelle `<Tmp>/a`, Ziel Ordner im Speicher, „Quelle → Ziel“, Löschmodus
   „Behalten“ (Vorgabe).
2. Vorschau öffnen: zeigt alle Dateien als Kopien. Speichern, „Jetzt abgleichen“: alles liegt im Speicher (SC-002).
3. In `a` eine Datei anlegen, eine ändern, eine umbenennen, eine löschen; abgleichen: neue und geänderte
   kommen an, die umbenannte wird verschoben (ohne erneutes Übertragen), die gelöschte bleibt im Speicher (FR-020).
4. Im Zielort eine Datei ändern: der Lauf überschreibt sie nicht, der Status meldet die Abweichung (FR-010).
5. Mitten in einem großen Kopieren die Vault sperren: kein Teilstück liegt im Ziel, der nächste Lauf setzt
   sauber fort (SC-005).

## 2. Beidseitig und Konflikt (Story 2)

1. Regel `<Tmp>/a` ↔ `<Tmp>/b`, „beidseitig“, Löschmodus „Spiegeln“.
2. In beiden Ordnern vorab gleichnamige Dateien: viele gleich, zwei verschieden. Erster Abgleich: gleiche werden
   übernommen ohne Kopie, die zwei verschiedenen ergeben `Name (Konflikt …)` auf beiden Seiten (FR-012a).
3. Dieselbe Datei auf beiden Seiten ändern: neuere behält den Namen, ältere liegt daneben (FR-012).
4. Datei links löschen, rechts ändern: die geänderte bleibt und kommt zurück (FR-013).
5. Datei links löschen, rechts unverändert: sie liegt rechts im Papierkorb des Systems (Desktop) oder in
   `.holzi-trash` (Android, Speicher) (FR-019).
6. Zweites Gerät: Regel `Laptop/Arbeit` ↔ `Telefon/Arbeit`; Telefon ausschalten → Regel „wartet“ ohne Fehler;
   einschalten → holt nach (FR-024).

## 3. Schutz und Kontrolle (Story 3)

1. Löschmodus „Nachfragen“, Datei links löschen: erscheint in der Liste, wird erst nach Bestätigung gelöscht.
2. Die Quelle unterbrechen (Ordner umbenennen oder Laufwerk abziehen): Lauf bricht mit „Ort nicht erreichbar“ ab,
   nichts wird gelöscht (FR-022, SC-004).

## 4. Ausnahmen und Protokoll (Story 4)

1. Regel → „Ausnahmen“: `*.tmp` und `node_modules/` eintragen, speichern. Die Datei `.haex-ignore` liegt in
   der Quelle; die Vorschau nennt die Zahl ausgenommener Dateien (FR-042).
2. Eine `.tmp`-Datei in einem Ordner anlegen: sie wird nicht kopiert; liegt sie schon an beiden Orten,
   bleibt sie unberührt (SC-009).
3. Regel → Protokoll: letzte Läufe mit Zählern; einen Lauf aufklappen: Pfad, Aktion, Seite. Einen
   gespiegelten Löscheintrag öffnen: führt in `.holzi-trash` oder nennt den Papierkorb des Systems.
4. Dateibrowser → Sync-Regeln → „Einstellungen“ (neben „Neue Regel“): Tiefe auf 2 setzen → ältere Läufe sind sofort weg; auf 0 → kein neues Protokoll.

## 5. Agent (Story 5)

1. Einstellungen → Agents → Dateien: „Sync-Regeln verwalten“ einschalten.
2. Agent bitten, eine Regel anzulegen: die Abfrage zeigt Orte, Richtung, Löschmodus; erst nach „Erlauben“
   entsteht sie. Ohne Berechtigung lehnt holzi ab.
