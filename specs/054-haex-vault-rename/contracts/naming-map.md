# Vertrag: Namenstabelle

Gilt für die Stufen S2–S7. Eine Regel pro Familie; Sonderfälle stehen unten und nirgends sonst.
Jede Stufe sagt, welche Familien sie anwendet (Spalte „Stufe“).

## Grundregel

Der Name in der Konvention der Umgebung. Die Version eines festen Textes (`/v1`, `/1`) ändert sich
**nicht**.

| Familie                                                  | alt                                                    | neu                                                                        | Stufe  |
| -------------------------------------------------------- | ------------------------------------------------------ | -------------------------------------------------------------------------- | ------ |
| Crate, Paket, Binärname, Pfadteil, Slug                  | `holzi`                                                | `haex-vault`                                                               | S4     |
| Rust-Bezeichner, Bibliothek                              | `holzi`, `holzi_lib`                                   | `haex_vault`, `haex_vault_lib`                                             | S4     |
| Rust-Typen mit Präfix                                    | `Holzi<X>`                                             | `Vault<X>` (bei Kollision `HaexVault<X>`)                                  | S2     |
| Rust-Bezeichner mit Teilwort                             | `…_holzi_…` (`pref_error_to_holzi`)                    | `…_haex_vault_…`                                                           | S2     |
| Umgebungsvariable                                        | `HOLZI_<X>`                                            | `HAEX_VAULT_<X>`                                                           | S4     |
| Systemeigenschaft                                        | `debug.holzi.probe`                                    | `debug.haex_vault.probe`                                                   | S4     |
| SQL-Name                                                 | `holzi_<x>`                                            | `haex_vault_<x>`                                                           | S2     |
| Kontaktart, Herkunft (Wert)                              | `holzi_<x>`, `holzi`                                   | siehe Sonderfälle                                                          | S2     |
| Schlüsselableitung, Hash, Signatur, Protokollname (Text) | `holzi/<x>/vN`, `holzi-<x>/vN`, `holzi-<x>/N`, `holzi` | `haex-vault/<x>/vN`, `haex-vault-<x>/vN`, `haex-vault-<x>/N`, `haex-vault` | S3     |
| UUID-Namensraum, Präfix (Text)                           | `holzi:<x>`, `holzi-<x>-vN`                            | `haex-vault:<x>`, `haex-vault-<x>-vN`                                      | S3     |
| Erweiterungs-Schema                                      | `holzi-ext`, `holzi-ext://…`, `…holzi-ext.localhost`   | `haex-ext`, `haex-ext://…`, `…haex-ext.localhost`                          | S5     |
| Android-Paket (App)                                      | `com.haex.holzi`                                       | `space.haex.vault`                                                         | S5     |
| Android-Paket (Plugin), Kotlin-Klasse                    | `space.haex.holzi.android`, `HolziAndroidPlugin`       | `space.haex.vault.android`, `HaexVaultAndroidPlugin`                       | S5     |
| Plugin-Crate, Rust-Erweiterung                           | `tauri-plugin-holzi-android`, `HolziAndroidExt`        | `tauri-plugin-haex-vault-android`, `HaexVaultAndroidExt`                   | S5     |
| CSS-Variable, -Klasse                                    | `--holzi-<x>`, `.holzi-<x>`                            | `--haex-vault-<x>`, `.haex-vault-<x>`                                      | S5     |
| Manifest-Schlüssel, MCP-Name, Dateiendung                | `x-holzi`, `holzi-approve`, `.holzi-part-`             | `x-haex-vault`, `haex-vault-approve`, `.haex-vault-part-`                  | S2     |
| Bilddatei, Bildordner                                    | `holzi-<x>.svg`, `holzi-states/`                       | `haex-vault-<x>.svg`, `haex-vault-states/`                                 | S2     |
| Anzeigetext                                              | `Holzi`, `holzi` im Satz, `holzi's`                    | `haex-vault`, `haex-vault's`                                               | S6     |
| Titel, Produktname, Version                              | `Holzi`/`holzi`, `0.1.0`                               | `haex-vault`, `3.0.0`                                                      | S6     |
| Kommentare und Doku                                      | beliebig                                               | `haex-vault`                                                               | S6, S7 |

## Sonderfälle

| Fundstelle                                         | Regel ergäbe                                | Wird                       | Grund                                              |
| -------------------------------------------------- | ------------------------------------------- | -------------------------- | -------------------------------------------------- |
| Kontaktart `holzi_vault`, Konstante `HOLZI_VAULT`  | `haex_vault_vault`, `HAEX_VAULT_VAULT`      | `haex_vault`, `HAEX_VAULT` | meint die Vault-Identität; kein Doppelwort         |
| Kontaktart `holzi_nostr_relay`, `holzi_iroh`       | `haex_vault_nostr_relay`, `haex_vault_iroh` | wie die Regel              | –                                                  |
| Herkunft `OriginKind::Holzi` (`'holzi'`)           | `HaexVault` / `haex_vault`                  | `HaexVault` / `haex_vault` | Variantenname trägt den Namen voll                 |
| Typpräfix bei Kollision mit vorhandenem `Vault<X>` | –                                           | `HaexVault<X>`             | der Compiler und die Suche zeigen es beim Anwenden |
| `HolziError::…` (Varianten)                        | unverändert                                 | unverändert                | nur der Typname ändert sich                        |
| Spec-Verzeichnis `032-model-operates-holzi`        | `…-haex-vault`                              | **bleibt**                 | FR-018                                             |
| Text „haex-vault“ **vor** S2                       | –                                           | „haex-vault 2.x (Legacy)“  | S1 (research.md, R4)                               |

## Was nicht in die Tabelle gehört

- Text, der die **alte** App meint (`haex_vault`, `haex-vault`): S1, nicht diese Tabelle.
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
