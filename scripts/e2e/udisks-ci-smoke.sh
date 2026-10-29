#!/usr/bin/env bash
# Spec 064 (research R12, task T033): checks on the CI runner that the runner user can drive
# udisks2 the way the drives scenario will: attach a loop image, mount, unmount, and unlock a
# LUKS2 image with a key file. Spike only; `scripts/e2e/lib/udisks.ts` takes over (T034).
set -euxo pipefail

dir="${RUNNER_TEMP:-/tmp}/udisks-smoke"
mkdir -p "$dir"

# `udisksctl loop-setup` prints "Mapped file … as /dev/loopN.", `unlock` "Unlocked /dev/loopN as /dev/dm-N."
device_of() { sed -n 's|.* as \(/dev/[a-z0-9-]*\)\.$|\1|p'; }

udisksctl status

truncate -s 32M "$dir/vfat.img"
mkfs.vfat -n SMOKEFAT "$dir/vfat.img"
loop=$(udisksctl loop-setup --no-user-interaction -f "$dir/vfat.img" | device_of)
udevadm settle
udisksctl mount --no-user-interaction -b "$loop"
udisksctl unmount --no-user-interaction -b "$loop"
udisksctl loop-delete --no-user-interaction -b "$loop"

truncate -s 64M "$dir/luks.img"
printf 'spike-only-password' > "$dir/key"
sudo cryptsetup luksFormat -q --type luks2 --pbkdf pbkdf2 --key-file "$dir/key" "$dir/luks.img"
loop=$(udisksctl loop-setup --no-user-interaction -f "$dir/luks.img" | device_of)
udevadm settle
clear=$(udisksctl unlock --no-user-interaction -b "$loop" --key-file "$dir/key" | device_of)
sudo mkfs.ext4 -q -L SMOKELUKS "$clear"
udevadm settle
udisksctl mount --no-user-interaction -b "$clear"
udisksctl unmount --no-user-interaction -b "$clear"
udisksctl lock --no-user-interaction -b "$loop"
udisksctl loop-delete --no-user-interaction -b "$loop"

rm -rf "$dir"
echo "udisks2 smoke passed"
