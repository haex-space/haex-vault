# Quickstart: Darstellung und Eingabefelder

Abnahme der Spec 035 von Hand und per Prüfung. Reihenfolge der Stufen: Felder (1), Kern der
Darstellung (2), COSMIC-Rest (3). Voraussetzungen: Worktree `035-ui-foundation`,
`nix develop`, gebaute App für e2e (siehe `docs`/Memory „e2e build gotchas“).

## Automatisch

```sh
nix develop --command pnpm check:appearance      # Farbrechnung, Ableitung, Kontrastmatrix, Datei
nix develop --command pnpm check:fields          # alte Feldarten nur in der Positivliste
nix develop --command pnpm check:settings
nix develop --command pnpm check:agent-actions
nix develop --command sh -c 'pnpm typecheck && pnpm typecheck:scripts && pnpm lint && pnpm format:check'
# e2e gegen die gebaute App
CARGO_TARGET_DIR=$PWD/src-tauri/target/e2e-build nix develop --command scripts/with-nix-host-bridge.sh pnpm tauri build --debug --no-bundle
nix develop --command pnpm test:e2e --app $PWD/src-tauri/target/e2e-build/debug/haex \
  --grep 'fields-basic|appearance-basic|appearance-sync-two-devices|settings-color-scheme|passwords-basic|settings-'
```

## Von Hand

### Stufe 1 – Felder (US1, SC-001, SC-002)

1. Neue Vault anlegen (Einrichtung): Alias- und Namensfeld mit schwebendem Label, Ring in
   Primärfarbe beim Klicken.
2. Passwortmanager: Eintrag anlegen; jedes Feld (Titel, Benutzername, Passwort, Adresse, Notiz,
   Tags, eigene Felder) schwebt beim Fokus; Passwort anzeigen, kopieren, leeren wie vorher.
3. Pflichtfeld leer lassen und speichern: Text unter dem Feld, Screenreader liest ihn.
4. Einstellungen → Allgemein: Gerätename-Feld in der Box; das Label unterbricht den Rand sauber.
5. Chat: Suche in der Seitenleiste, Modellauswahl, Umbenennen eines Threads.
6. Fenster auf 360 px verkleinern: Labels werden abgeschnitten, Felder bleiben bedienbar.

### Stufe 2 – Kern der Darstellung (US2–US5, SC-003 bis SC-007)

1. Einstellungen → Darstellung: Reihe Akzentfarben; Blau wählen: Ring, Schalter, Knöpfe,
   Auswahlmarkierung wechseln sofort in allen offenen Fenstern; App neu starten: bleibt.
2. „+“ → eigene Farbe (zum Beispiel `#ff00aa`): gilt sofort, steht als weiteres Feld in der Reihe.
3. Fenster- und Container-Hintergrund mit „Warm“ und „Kühl“: nur die jeweiligen Flächen ändern
   sich; Hell und Dunkel bleiben hell und dunkel.
4. Sehr hellen Akzent (`#ffffaa`) wählen: Beschriftung auf Knöpfen bleibt lesbar; die Zeile
   zeigt „angepasst“ mit Grund.
5. Schema auf „Automatisch“, System von Hell auf Dunkel umschalten: HaexVault folgt; Akzent bleibt.
6. „Auf Standard zurücksetzen“ → Bestätigung → alle Werte zurück, Schema unverändert.
7. Exportieren, zurücksetzen, die Datei importieren: derselbe Zustand. Eine Datei mit
   `"accent":{"preset":"nope"}` importieren: Meldung nennt das Feld, nichts ändert sich.
8. Zwei Geräte (Spec 033): auf A die Farbe ändern; B zeigt sie nach dem nächsten Sync ohne
   Neustart.
9. Anderes Vault öffnen (Neustart): zeigt nie die Farben der vorherigen.

### Stufe 3 – COSMIC-Rest (FR-013 Rest, FR-024)

1. Texttönung und Komponententönung ändern; Suchfelder, Schaltflächen und Text ändern sich, alles
   lesbar.
2. „Akzentfarbe als Hinweis für das aktive Fenster“ einschalten: nur das aktive Fenster hat die
   Akzent-Umrandung; Fenster wechseln, die Umrandung wandert mit.

## Abnahmekriterien

- Alle Prüfungen oben grün; die Kontrastmatrix (`check:appearance`) meldet keine Verletzung.
- Eine Durchsicht per `git grep` findet kein Feld in alter Feldart außerhalb der Positivliste.
- Keine Speichern- oder Übernehmen-Knöpfe; „Auf Standard zurücksetzen“ wirkt erst nach
  Bestätigung.
