# Contract: `platform_capabilities`

**Anforderungen**: FR-016, FR-026, FR-011a, FR-013 | **Research**: R9

## Command

```text
platform_capabilities() -> PlatformCapabilities
```

- Kein Tresor nötig; aufrufbar vor dem Entsperren (die Tresorauswahl braucht `folderPick`
  nicht, die Zurück-Geste aber `backGesture`).
- Ergebnis ist für die Lebensdauer des Prozesses fest; die Oberfläche liest es einmal beim
  Start (`useDeviceCapabilities()`) und hält es im Speicher.
- Felder und Werte: [data-model.md](../data-model.md#platformcapabilities-fr-016-fr-026-vertrag-platform-capabilitiesmd).

## Regeln im Kern

- Die Tabelle ist die einzige Stelle, die entscheidet, ob eine Fähigkeit auf diesem Gerät
  existiert. `extensions/shell` (`desktop_only()`), `extensions/fs` (`free_paths`, Ordner
  beobachten, Ordnerauswahl), die Delegate-Schicht, die Werkzeug-Anmeldung im Chat und die
  Hardware-Erkennung lesen sie; eigene `cfg!(desktop)`-Abfragen für diese Fragen entfallen.
- `cfg` bleibt nur dort, wo Code auf einer Plattform gar nicht übersetzt (z. B.
  `blocking_pick_folder`); der Zweig für die andere Plattform antwortet dann aus der Tabelle.
- Fehlende Fähigkeit:
  - Erweiterungs-Bridge: `ExtensionErrorCode::NotAvailable` (8001), wie heute.
  - Commands der Oberfläche: `HaexError::NotAvailable` mit dem Text „Auf diesem Gerät nicht
    verfügbar“ (i18n-Schlüssel `errors.notAvailable`).
  - Delegates: `AdapterError::Unavailable` mit Grund `platform` (nicht „nicht installiert“).

## Regeln in der Oberfläche

- Angebote einer fehlenden Fähigkeit werden ausgeblendet: Delegates in der Modellwahl
  (`useModelInventory.ts`) und unter Einstellungen → Agenten; Entwicklungsmodus der
  Erweiterungen; Ordnerauswahl in Erweiterungs-Dialogen.
- Wo ein Ausblenden Gespeichertes verstecken würde (per Sync übernommene Delegate-Einstellungen),
  zeigt die Oberfläche den Eintrag mit dem Hinweis „Auf diesem Gerät nicht verfügbar“ statt ihn
  wegzulassen.
- Die Android-Erkennung über den User-Agent in `src/plugins/actions.client.ts` entfällt;
  `backGesture` entscheidet.

## Tests

- Rust: Tabelle pro `cfg` (Desktop-Test im normalen Lauf, Android-Werte als Konstante geprüft);
  jede Leser-Stelle hat einen Test, dass sie bei `false` „nicht verfügbar“ liefert.
- e2e Android: Gegenfälle der Ausnahmeliste ([e2e-android.md](./e2e-android.md)).

## Umsetzung in Stufe 5

- Neues Feld `modelPresets` (`'desktop' | 'phone'`, FR-027, FR-030): Unter `phone` schlägt der
  Modellkatalog nur die beiden Telefon-Profile aus `_meta.profiles` vor (`catalog/profiles.rs`),
  zwischen denen die Passung zum Arbeitsspeicher entscheidet, und der Vorschlag der
  Spracherkennung ist das kleinste Modell (`whisper-tiny`); die größeren bleiben wählbar.
