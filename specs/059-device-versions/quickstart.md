# Quickstart: Versionsstände der Geräte prüfen

Voraussetzung: Entwicklungsumgebung wie in `CLAUDE.md` des Repos (`nix develop`, `pnpm install`). Cargo nur
gezielt und mit `-j 4`.

## 1. Automatische Prüfungen

```fish
nix develop --command cargo test -j 4 --lib sync::handshake_schema_tests sync::peer_schema_tests sync::device_view
nix develop --command cargo test -j 4 --lib identity::migrations_peer_schema
pnpm check:settings   # scripts/check-settings.ts prüft deviceStatus.ts, hier um schemaDiff erweitert
```

Erwartet: alle grün. Der Handshake-Test deckt ab, dass beide Seiten nach `Accept` und nach `Reject
{ Incompatible }` die Werte des anderen kennen und dass ein altes `v = 1` als `Incompatible` endet.

## 2. Von Hand mit zwei Geräten (M1)

1. Zwei Geräte derselben Vault auf demselben Stand starten. Einstellungen → Föderation → Geräte: bei beiden
   Zeilen stehen Sync-Protokoll, Vault-Schema, Änderungsverfolgung; nichts ist markiert.
2. Auf einem Gerät holzi auf einen älteren Stand zurücksetzen (anderer Checkout) und starten. Auf dem neuen Gerät
   erscheint die rote Versionsmeldung, und die Werte des alten sind markiert: kleiner bei Vault-Schema.
3. Auf dem alten Gerät sieht die Liste (alter Stand kennt die Werte noch nicht) nur die Meldung; nach dem
   Aktualisieren zeigt sie beide Stände.
4. Altes Gerät aktualisieren und verbinden: Meldung und Markierung verschwinden ohne die Ansicht neu zu öffnen
   (SC-002).

## 3. Schmaler Bildschirm (M2)

Fenster auf Telefonbreite ziehen (oder Android-Emulator): jede Zeile umbricht, kein Seitwärtsscrollen (SC-004).

## 4. E2E

```fish
nix develop --command scripts/with-nix-host-bridge.sh pnpm test:e2e --grep federation
# Szene settings-federation: „Dieses Gerät“ zeigt drei Zahlen, nichts ist markiert
```
