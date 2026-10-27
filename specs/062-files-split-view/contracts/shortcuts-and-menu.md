# Vertrag: Tasten, Menü, Beschriftungen

Gilt nur mit sichtbarem Split (zwei Panels). Ohne Split bleibt Spec 044 FR-017b unverändert.

## Tasten (im Panel, Fokus in der Liste, nicht in Textfeld/Dialog/auf markiertem Text)

| Taste                    | Befehl        | Wirkung                                                                     |
| ------------------------ | ------------- | --------------------------------------------------------------------------- |
| `Tab`                    | `switchPanel` | Das andere Panel wird aktiv, Fokus auf dessen zuletzt fokussierten Eintrag. |
| `Umschalt+Tab`           | –             | Browser-Standard (aus der Liste rückwärts heraus).                          |
| `F5`                     | `copyToOther` | Auswahl in den Ordner des Gegenpanels kopieren. Ohne Auswahl: nichts.       |
| `F6`                     | `moveToOther` | Auswahl in den Ordner des Gegenpanels verschieben. Ohne Auswahl: nichts.    |
| `Strg+R` / `Cmd+R`       | `reload`      | Neuladen (nur noch so; `F5` ist in der Split-Ansicht Kopieren).             |
| Übrige Kürzel (Spec 044) | unverändert   | Wirken auf das Panel mit dem Fokus.                                         |

`F5`/`F6` laufen durch dieselbe Prüfung wie das Menü (`runKey`): was das Menü sperrt (schreibgeschütztes
Ziel, eigene Daten von holzi, Verschieben aus einem schreibgeschützten Panel), sperrt auch die Taste und
zeigt den Grund als Meldung.

## Menü (Kontextmenü und Aktionsleiste)

Zwei neue Befehle zwischen „Ausschneiden/Kopieren“ und „Einfügen“, nur mit Split:

| Id            | de                              | en                  | Kürzel |
| ------------- | ------------------------------- | ------------------- | ------ |
| `copyToOther` | In das andere Panel kopieren    | Copy to other panel | F5     |
| `moveToOther` | In das andere Panel verschieben | Move to other panel | F6     |

## Werkzeugleiste

| Element                       | Beschriftung                 | Verhalten                                                                                                                          |
| ----------------------------- | ---------------------------- | ---------------------------------------------------------------------------------------------------------------------------------- |
| Schalter `files-split-toggle` | „Zwei Panels“ / „Two panels“ | `aria-pressed`, ein/aus; deaktiviert mit Grund „Das Fenster ist zu schmal für zwei Panels“ bei `!fits`; fehlt auf Android und iOS. |

## Meldungen

`files.error.sameFolder`: „Quelle und Ziel sind derselbe Ordner.“ / „Source and target are the same
folder.“ (bei F6; F5 geht in die Namenskonflikt-Frage). Verweigertes Ziel nutzt die vorhandenen
Meldungen (`holziOwned`, `intoItself`, Schreibschutz einer Freigabe).
