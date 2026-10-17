# Quickstart: holzi für Android prüfen

**Spec**: [spec.md](./spec.md) | **Plan**: [plan.md](./plan.md)

Jeder Abschnitt nennt die Stufe, ab der er gilt. Automatisch geprüft wird alles, was die
e2e-Suite im Emulator kann ([contracts/e2e-android.md](./contracts/e2e-android.md)); von Hand
auf einem echten Telefon bleibt, was der Emulator nicht kann.

## §0 Voraussetzungen

- Nix-devShell mit der Android-Umgebung aus den gepinnten atoms-Dateien
  ([`haexmas/atoms` @ `b662c6205d6a1ebd7fac588291f82ea8e78c3d52`](https://github.com/haexmas/atoms/tree/b662c6205d6a1ebd7fac588291f82ea8e78c3d52),
  holzi PR #308):
  `nix develop` setzt `ANDROID_HOME`, `NDK_HOME`, `ANDROID_NDK_ROOT` und `JAVA_HOME`; die
  Rust-Toolchain bringt die Android-Ziele mit, `rustup` ist nicht nötig.
- Ein Telefon mit Entwickleroptionen und USB-Debugging, oder ein Emulator (API 35, x86_64).
- `pnpm install` im Worktree.

## §1 Bauen und starten (Stufe 1a)

```sh
nix develop --command pnpm tauri android build --debug --apk --split-per-abi --target aarch64 --target x86_64
adb install -r src-tauri/gen/android/app/build/outputs/apk/arm64/debug/app-arm64-debug.apk     # Telefon
adb install -r src-tauri/gen/android/app/build/outputs/apk/x86_64/debug/app-x86_64-debug.apk   # Emulator
```

Erwartet: Das APK installiert sich, holzi startet, die Tresorauswahl erscheint in ≤ 3 s
(SC-001). Tresor anlegen, einen Eintrag anlegen, Tresor schließen → die App endet (FR-006);
neu starten → Tresorauswahl; entsperren ≤ 10 s (SC-002), der Eintrag ist da.

Alternativ das APK aus dem CI-Lauf des PRs (Artefakt `holzi-android-debug`).

## §2 Entwicklungsmodus (Stufe 1a)

```sh
nix develop --command pnpm tauri android dev            # Emulator
nix develop --command pnpm tauri android dev --host     # echtes Telefon im selben Netz
```

Erwartet: holzi startet mit dem Dev-Server; eine Änderung an einer `.vue`-Datei erscheint ohne
neues APK (FR-034).

## §3 Telefon-Oberfläche, von Hand (Stufe 1c)

Auf einem echten Telefon (Bildschirmbreite auf 360 dp stellen, z. B. Anzeigegröße „groß“):

1. Spec 036 T081: zwischen Details, Extra und Verlauf wischen; Zeile lange drücken → Auswahl;
   Zeilenmenü-Knopf; Anhang antippen → Lightbox, Zwei-Finger-Zoom, Wischen, Zurück schließt.
2. Nichts liegt unter Statusleiste, Navigationsleiste oder Kamera-Aussparung; nichts scrollt
   waagrecht (FR-010, FR-011).
3. Ein Feld unten antippen → es bleibt über der Tastatur sichtbar (FR-012).
4. Zurück-Geste: Tab zurück → Fensterübersicht → schließen; die App wird nie verlassen
   (FR-013).
5. Telefon drehen, geteilter Bildschirm: Sitzung bleibt, kein erneutes Entsperren (FR-009).
6. App-Übersicht öffnen → Vorschau leer; Bildschirmfoto → verweigert; Einstellungen →
   Allgemein → Bildschirmschutz aus → beides geht wieder (FR-011a).
7. Dateiauswahl: Anhang aus „Downloads“ und aus einem Cloud-Anbieter hinzufügen; Darstellung
   exportieren und wieder importieren; Import aus haex-vault 2.x (Legacy) (FR-015).
8. Tresordatei öffnen: Eine Tresordatei vom Desktop in „Downloads“ legen, in der Tresorauswahl
   „Tresordatei öffnen“ → Datei wählen → Passwort → Tresor offen; dieselbe Datei noch einmal →
   „gibt es schon“ (FR-002a).
9. App aus der App-Übersicht wegwischen, neu starten → Tresorauswahl, Tresor entsperrbar ohne
   „anderswo geöffnet“ (FR-007).
10. Ordner wählen in einer Erweiterung, Terminal, Delegates: „Auf diesem Gerät nicht verfügbar“
    bzw. ausgeblendet; nichts stürzt ab (FR-016).

Ergebnis in Spec 036 T081 und in dieser Spec eintragen.

## §4 e2e im Emulator (Stufe 1b)

```sh
emulator -avd holzi-api35 -no-window -gpu swiftshader_indirect -noaudio &
adb wait-for-device
adb install -r <x86_64-Debug-APK>
nix develop --command pnpm test:e2e --platform android
nix develop --command pnpm check:e2e-exclusions
```

Erwartet: alle Szenarien außer `excluded` und `pending` grün; `check:e2e-exclusions` meldet die
Abdeckung (Ziel 95 %, mindestens 80 %).

## §5 Sync mit dem Desktop (Stufe 2)

1. Desktop und Telefon koppeln (wie zwischen zwei Desktops); beide Geräte in beiden Listen.
2. Eintrag am Desktop ändern → am Telefon in ≤ 10 s sichtbar; umgekehrt (SC-005).
3. holzi am Telefon in den Hintergrund, am Desktop ändern, holzi wieder öffnen → Änderung kommt
   (FR-018).
4. Während eines Syncs WLAN aus- und Mobilfunk einschalten → Sync setzt fort (FR-019).

## §6 Erweiterungen (Stufe 3)

1. Erweiterung aus „Downloads“ installieren, Berechtigung erteilen, benutzen (FR-020).
2. `extension-isolation` im Emulator grün (FR-021, Spec 017 T112).
3. Erweiterung lässt eine Datei öffnen und speichern → System-Dateiauswahl (FR-022).
4. Benachrichtigung: Berechtigung erlauben → Android-Benachrichtigung; ablehnen → Meldung in
   holzi, kein Absturz (FR-023).

## §7 Chat mit Online-Anbietern (Stufe 4)

1. Anbieter mit API-Schlüssel einrichten, Frage stellen, gestreamte Antwort (FR-025).
2. Basis-URL auf einen Server mit selbst signiertem Zertifikat setzen → Fehlermeldung zum
   Zertifikat, keine Verbindung (FR-024).
3. Modellwahl: keine Delegates claude/codex (FR-026).

## §8 Lokale KI und Sprache (Stufe 5)

1. Modellwahl auf einem Telefon mit ≥ 6 GB RAM: Vorschlag ist Qwen3-1.7B; Downloadgröße und
   freier Speicher werden vor dem Download gezeigt, Download erst nach Bestätigung (FR-027,
   FR-028).
2. Flugmodus an, Frage stellen → Antwort beginnt in ≤ 15 s (SC-009); Messwert eintragen.
3. Spracheingabe: Mikrofon-Abfrage beim ersten Mal; erlauben → Diktat erscheint im Feld;
   ablehnen → Erklärung, wie man es später erlaubt (FR-029). Vorschlag ist `whisper-tiny`
   (FR-030).

## §9 Release (Stufe 1d)

1. Tag `v<x.y.z>` auf einen Testzweig im Fork setzen (oder `workflow_dispatch`), Workflow
   `android-release` läuft, `apksigner verify --print-certs` zeigt den erwarteten Fingerabdruck.
2. Release-APK der Vorversion installieren, Tresor anlegen, neues Release-APK darüber
   installieren → Tresor und Einstellungen sind da (FR-033, SC-008).
