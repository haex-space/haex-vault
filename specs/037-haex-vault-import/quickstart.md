# Quickstart: Passwörter aus haex-vault 2.x (Legacy) übernehmen

## Voraussetzungen

- Worktree von holzi mit Nix-Devshell; cargo läuft nur über
  `nix develop --command scripts/with-nix-host-bridge.sh …`.
- Für §3: eine echte Vault-Datei von haex-vault 2.x @ `8dce379` mit Passwörtern und ihr Vault-Passwort.

## §1 Automatische Prüfung (Rust)

```sh
nix develop --command scripts/with-nix-host-bridge.sh \
  cargo test --manifest-path src-tauri/Cargo.toml -j 4 \
  --test passwords_import_legacy_haex_vault --test passwords_import --test passwords_import_keepass
nix develop --command scripts/with-nix-host-bridge.sh \
  cargo test --manifest-path src-tauri/Cargo.toml -j 4 --lib passwords::import
```

Erwartet: alle grün. Abgedeckt sind:

- jedes Feld aus [contracts/haex-vault-mapping.md](contracts/haex-vault-mapping.md) (SC-001);
- die Quelldatei und `-wal` sind nach Vorschau, Import, falschem Passwort und Abbruch Byte für Byte
  gleich (SHA-256 vorher und nachher, SC-002);
- ein zweiter Import mit `skip` legt nichts an (SC-003), auch keine Ordner, auch für KeePass;
- Änderungen nur in `-wal` kommen an;
- die Fehlergründe `legacy_haex_vault_locked`, `unsupported_format`, `no_passwords`.

Danach `git checkout -- src/types/bindings/` nur für Bindings, die sich bloß im Leerraum geändert
haben; die neuen Bindings (`ImportSource`, `ImportPreview`, `AttentionKind`) werden mit
`sed -E 's/[[:space:]]+$//'` bereinigt und committet.

## §2 CI-Gleichstand

`cargo fmt --check`, `pnpm lint:rust`, `pnpm lint`, `pnpm typecheck`, `pnpm format:check`,
`pnpm check:templates`, `pnpm check:passwords` — alle über `nix develop --command`.

## §3 Manuell mit der echten Vault (Operator)

1. haex-vault 2.x auf dem alten Gerät schließen, `<name>.db` (und `<name>.db-wal`, falls vorhanden)
   auf dieses Gerät kopieren. SHA-256 der Dateien notieren.
2. holzi starten, Passwortmanager → Import → Quelle „haex-vault 2.x“, Datei wählen.
3. Falsches Passwort eingeben → Vorschau meldet „Vault-Passwort passt nicht oder keine
   haex-vault-2.x-Vault“, nichts geschrieben.
4. Richtiges Passwort → Vorschau: Zahlen mit haex-vault 2.x vergleichen (Einträge, Ordner, Papierkorb,
   Tags, Anhänge, Verlauf, Passkeys, Voreinstellungen); Hinweis zum Schließen von haex-vault 2.x ist da.
5. Import starten → Bericht lesen; jede genannte Stelle in holzi prüfen.
6. Stichproben: drei Einträge mit TOTP (Code stimmt mit haex-vault 2.x überein), ein Anhang öffnen
   (Inhalt gleich), ein eigenes Bild, ein Eintrag im Papierkorb, Farbe von Eintrag, Ordner und Tag,
   ein Verlaufsstand mit Anhang, ein Passkey (Relying Party, Nutzer).
7. Import wiederholen mit „überspringen“ → 0 neue Einträge, keine doppelten Ordner.
8. SHA-256 der kopierten Dateien erneut bilden → unverändert.
