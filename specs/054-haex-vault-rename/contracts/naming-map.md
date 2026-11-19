# Vertrag: Namenstabelle

Gilt für die Stufen S2–S7. Eine Regel pro Familie; Sonderfälle stehen unten und nirgends sonst.
Jede Stufe sagt, welche Familien sie anwendet (Spalte „Stufe“).

## Grundregel

Das Wort `holzi` wird durch `haex` ersetzt, sonst ändert sich nichts am Namen: `holzi_migration_source`
wird `haex_migration_source`, `HolziError` wird `HaexError`, `holzi-trash` wird `haex-trash`,
`HOLZI_PROBE` wird `HAEX_PROBE`. Nur wo von der Vault selbst die Rede ist, steht „HaexVault“ (Anzeigename
der App) oder `haex_vault` (zum Beispiel die Kontaktart `holzi_vault`, die Identität der Vault); Repo,
Identifier und Verzeichnisse heißen `haex-vault` (Betreiber, 2026-11-19). Die Version eines festen Textes
(`/v1`, `/1`) ändert sich **nicht**.

| Familie                                                  | alt                                                                            | neu                                                                          | Stufe  |
| -------------------------------------------------------- | ------------------------------------------------------------------------------ | ---------------------------------------------------------------------------- | ------ |
| Rust-Typen, Felder, Funktionen, Konstanten, Testnamen    | `Holzi<X>`, `holzi_<x>`, `<x>_holzi`, `HOLZI_<X>`                              | `Haex<X>`, `haex_<x>`, `<x>_haex`, `HAEX_<X>`                                | S2     |
| SQL-Name, Kontaktart, Herkunft (Wert)                    | `holzi_<x>`, `holzi`                                                           | `haex_<x>`, `haex`                                                           | S2     |
| Manifest-Schlüssel, MCP-Name, Dateiendung, MIME, Ordner  | `x-holzi`, `holzi-approve`, `.holzi-part-`                                     | `x-haex`, `haex-approve`, `.haex-part-`                                      | S2     |
| Bilddatei, Bildordner                                    | `holzi-<x>.svg`, `holzi-states/`                                               | `haex-<x>.svg`, `haex-states/`                                               | S2     |
| Schlüsselableitung, Hash, Signatur, Protokollname (Text) | `holzi/<x>/vN`, `holzi-<x>/vN`, `holzi-<x>/N`, `holzi`                         | `haex/<x>/vN`, `haex-<x>/vN`, `haex-<x>/N`, `haex`                           | S3     |
| UUID-Namensraum, Präfix (Text)                           | `holzi:<x>`, `holzi-<x>-vN`                                                    | `haex:<x>`, `haex-<x>-vN`                                                    | S3     |
| Crate, Paket, Binärname, Bibliothek, Pfadteil            | `holzi`, `holzi_lib`                                                           | `haex`, `haex_lib`                                                           | S4     |
| Umgebungsvariable, Systemeigenschaft                     | `HOLZI_<X>`, `debug.holzi.probe`                                               | `HAEX_<X>`, `debug.haex.probe`                                               | S4     |
| Erweiterungs-Schema                                      | `holzi-ext`, `holzi-ext://…`, `…holzi-ext.localhost`                           | `haex-ext`, `haex-ext://…`, `…haex-ext.localhost`                            | S5     |
| Android-Paket (App)                                      | `com.haex.holzi`                                                               | `space.haex.vault`                                                           | S5     |
| Android-Paket (Plugin), Kotlin-Klasse, Plugin-Crate      | `space.haex.holzi.android`, `HolziAndroidPlugin`, `tauri-plugin-holzi-android` | `space.haex.vault.android`, `HaexAndroidPlugin`, `tauri-plugin-haex-android` | S5     |
| CSS-Variable, -Klasse                                    | `--holzi-<x>`, `.holzi-<x>`                                                    | `--haex-<x>`, `.haex-<x>`                                                    | S5     |
| Anzeigetext, Titel, Produktname, Version                 | `Holzi`, `holzi` im Satz, `0.1.0`                                              | `HaexVault`, `3.0.0`                                                         | S6     |
| Kommentare und Doku                                      | beliebig                                                                       | `haex` (die App als Ganzes: `HaexVault`)                                     | S6, S7 |

## Sonderfälle

| Fundstelle                                             | Wird                            | Grund                                                                          |
| ------------------------------------------------------ | ------------------------------- | ------------------------------------------------------------------------------ |
| Kontaktart `holzi_vault`, Konstante `HOLZI_VAULT`      | `haex_vault`, `HAEX_VAULT`      | die Regel ergibt es von selbst; sie meint die Identität der Vault              |
| Kontaktart `holzi_nostr_relay`, `holzi_iroh`           | `haex_nostr_relay`, `haex_iroh` | wie die Regel                                                                  |
| `HolziAndroid…` (Plugin)                               | S5                              | das Plugin zieht mit seinem Crate um                                           |
| Spec-Verzeichnis `032-model-operates-holzi`            | **bleibt**                      | FR-018                                                                         |
| Text „haex-vault“ **vor** S2                           | „haex-vault 2.x (Legacy)“       | S1 (research.md, R4)                                                           |
| Migrations-SQL, das den alten Namen trägt (0020, 0042) | **bleibt**                      | haex-crdt prüft angewendete Migrationen nach Inhalt; Migration 0065 benennt um |

## Was nicht in die Tabelle gehört

- Text, der die **alte** App meint (`haex_vault`, `haex-vault` vor S1): S1, nicht diese Tabelle.
- Der Text der Spec 054 selbst und der Hinweissatz im README: Ausnahmeliste, nicht umbenannt.
- Dateien, die ein Werkzeug erzeugt (`Cargo.lock`, `src/types/bindings/*`,
  `src-tauri/gen/android/**` teilweise): nach der Änderung der Quelle neu erzeugt, nicht ersetzt.

## Anwenden

1. Familien der Stufe aus der Tabelle wählen, Muster **mit** Wortgrenzen und Groß-/Kleinschreibung
   formulieren (`\bholzi\b`, `HOLZI_`, `Holzi[A-Z]`), niemals ein blindes `s/holzi/…/`.
2. Zuerst Sonderfälle, dann Familien, dann Pfade (`git mv`), dann den Rest.
3. Erzeugte Dateien neu erzeugen (`cargo test` mit ts-rs, `tauri` für `gen/android`).
4. Suche nach der Stufe: `git grep -n -i holzi -- <Gebiet der Stufe>` ist leer, außer den Treffern
   anderer Stufen.
5. Test der Stufe: Gebiet bauen (nur die betroffenen Ziele, `-j 4`), Test des Testvaults und die
   Prüfung der Stufe aus [quickstart.md](../quickstart.md).

Löst ein Konflikt mit parallel gemergten Änderungen auf, wird Schritt 1–3 erneut auf dem neuen
Stand ausgeführt, statt Konfliktmarker von Hand zu lösen.
