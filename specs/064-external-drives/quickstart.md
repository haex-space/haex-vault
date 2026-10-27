# Quickstart: Externe Laufwerke prüfen

Manuelle Prüfung je Plattform; die automatischen Tests stehen im Plan. Vorher die App neu bauen: Rust
hat kein Hot-Reload.

## Linux (udisks2)

Vorbereitung:

- ein USB-Stick mit FAT/exFAT und Bezeichnung (z. B. `FOTOS`)
- ein LUKS-Stick. Ohne echten zweiten Stick geht ein Image:

```sh
loop_device=
cleanup() {
  sudo systemctl restart udisks2 || true
  if test -n "$loop_device"; then
    udisksctl loop-delete -b "$loop_device" || true
  fi
  rm -f /tmp/luks.img
}
trap cleanup EXIT

truncate -s 64M /tmp/luks.img
sudo cryptsetup luksFormat -q --type luks2 /tmp/luks.img   # Passwort z. B. "geheim"
loop_device=$(udisksctl loop-setup -f /tmp/luks.img | sed -n 's/.* as \(\/dev\/[^. ]*\)\.$/\1/p')
test -n "$loop_device"
mapper=$(udisksctl unlock -b "$loop_device" | sed -n 's/.* as \(\/dev\/[^. ]*\)\.$/\1/p')
test -n "$mapper"
sudo mkfs.ext4 -L BACKUP "$mapper"                    # … und formatieren
mountpoint=$(udisksctl mount -b "$mapper" | sed -n 's/.* at \(.*\)\.$/\1/p')
printf 'test file\n' | sudo tee "$mountpoint/x.txt" >/dev/null
udisksctl unmount -b "$mapper"
udisksctl lock -b "$loop_device"
```

1. **US1**:
   - Dateibrowser öffnen und den Stick einstecken. Innerhalb von 3 s steht `FOTOS` mit USB-Symbol
     unter „Laufwerke“ (SC-001).
   - Hat der Desktop ihn nicht selbst eingehängt: anklicken. Der Inhalt erscheint (SC-002).
   - Abziehen. Der Eintrag verschwindet, ein Tab darauf springt in den nächsten vorhandenen Ordner und
     zeigt den Hinweis.
2. **US1/6**: `/boot/efi`, `/recovery` und Swap stehen nicht in der Liste; `/` steht dort ohne Knopf.
3. **US2**:
   - Ein Video vom Stick im Viewer öffnen, dann „Auswerfen“. Das gelingt beim ersten Versuch mit der
     Meldung „kann abgezogen werden“ (SC-004).
   - Mit einer laufenden großen Kopie auf den Stick kommt erst die Rückfrage.
   - Mit `less /run/media/$USER/FOTOS/x.txt` in einem Terminal kommt „wird noch … benutzt“.
4. **US3**:
   - Das LUKS-Image erscheint mit Schloss. Klick → Dialog.
   - Falsches Passwort: Der Dialog bleibt offen.
   - Richtiges Passwort ohne „merken“: `BACKUP` öffnet. Im Passwortmanager gibt es keinen neuen
     Eintrag.
5. **US4**:
   - Auswerfen, erneut klicken, mit „merken“ entsperren. Im Passwortmanager steht „Laufwerk: BACKUP“
     mit URL `haex:drive:<uuid>` und Tag `haex-drive`.
   - Auswerfen und wieder klicken: Es öffnet ohne Dialog.
   - Auf einem zweiten eigenen Gerät nach dem Sync dasselbe (SC-003).
6. **FR-017**: Während des Entsperrens `ps aux | grep -i '[g]eheim'`: kein Treffer. Im Log (`RUST_LOG=debug`)
   steht das Passwort nicht.
7. **FR-025**: Tab auf `BACKUP/ordner` lassen, haex beenden, das Dateisystem aushängen und das Image
   sperren. haex starten: Der Tab
   zeigt „ist gesperrt“ mit „Entsperren“. Danach ist `ordner` offen.
8. **FR-024**: `systemctl stop udisks2` (mit sudo), haex neu starten. Eingehängte Laufwerke stehen in
   der Liste, ohne Knöpfe.

## macOS

- USB-Stick (APFS oder exFAT) einstecken: Er erscheint mit seinem Label, lässt sich öffnen und über
  den vorgesehenen Auswerfen-Knopf sicher entfernen.
- Ein verschlüsseltes APFS-Volume (Festplattendienstprogramm → „APFS (verschlüsselt)“) erscheint
  gesperrt, lässt sich über den Entsperrdialog öffnen und danach sicher auswerfen. Während des
  Entsperrens zeigt `ps -ef | grep '[d]iskutil'` kein Passwort.

## Windows

- USB-Stick: erscheint mit Laufwerksbuchstaben und USB-Symbol; „Auswerfen“ gelingt. Interne
  Laufwerke haben keinen Knopf.
- BitLocker-To-Go-Stick: Ein Klick öffnet den Entsperrdialog von Windows (FR-019). Nach dem
  Entsperren dort lässt sich der Inhalt in haex öffnen.

## Android

- OTG-Stick einstecken: Er erscheint ohne Neuöffnen in der Liste und lässt sich öffnen. Es gibt keine
  Knöpfe.
