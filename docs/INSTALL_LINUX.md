# Installing Haex Vault on Linux

Haex Vault ships native packages for Debian/Ubuntu (`.deb`),
Fedora/RHEL/openSUSE (`.rpm`), and Arch Linux (`.pkg.tar.zst`) via our own
package repositories. Once configured, future updates arrive through your
system's package manager — `apt upgrade` / `dnf upgrade` / `pacman -Syu` —
just like any other system package.

> **Supported architectures**: `amd64` / `x86_64` and `arm64` / `aarch64`.
> Your package manager picks the right one automatically.

## Debian / Ubuntu (apt)

```bash
# 1. Trust the repo signing key
sudo install -d -m 0755 /etc/apt/keyrings
curl -fsSL https://apt.haex.space/pubkey.gpg \
  | sudo tee /etc/apt/keyrings/haex-vault.asc > /dev/null

# 2. Add the repository
echo "deb [signed-by=/etc/apt/keyrings/haex-vault.asc] https://apt.haex.space stable main" \
  | sudo tee /etc/apt/sources.list.d/haex-vault.list > /dev/null

# 3. Install
sudo apt update
sudo apt install haex-vault
```

Future updates:

```bash
sudo apt update && sudo apt upgrade
```

## Fedora / RHEL / Rocky / Alma (dnf / yum)

```bash
# 1. Trust the repo signing key
sudo rpm --import https://rpm.haex.space/pubkey.gpg

# 2. Add the repository
sudo tee /etc/yum.repos.d/haex-vault.repo > /dev/null <<'EOF'
[haex-vault]
name=Haex Vault
baseurl=https://rpm.haex.space/
enabled=1
gpgcheck=1
repo_gpgcheck=1
gpgkey=https://rpm.haex.space/pubkey.gpg
EOF

# 3. Install
sudo dnf install haex-vault
```

Future updates: handled automatically by `dnf upgrade`.

## openSUSE (zypper)

```bash
sudo rpm --import https://rpm.haex.space/pubkey.gpg
sudo zypper addrepo --gpgcheck --refresh \
  https://rpm.haex.space/ haex-vault
sudo zypper install haex-vault
```

## Arch / Manjaro / EndeavourOS (pacman)

The signing key fingerprint is
`92B1 6ADF 139D F0D5 BA0B  2C8A 7940 193A 39D0 D4EA` — compare it against
the output of `gpg --show-keys` below before lsigning.

```bash
# 1. Trust the repo signing key
curl -fsSL https://arch.haex.space/pubkey.gpg -o /tmp/haex.gpg
gpg --show-keys /tmp/haex.gpg   # verify the fingerprint matches the one above
sudo pacman-key --add /tmp/haex.gpg
sudo pacman-key --lsign-key 92B16ADF139DF0D5BA0B2C8A7940193A39D0D4EA
rm /tmp/haex.gpg

# 2. Add the repository to /etc/pacman.conf
sudo tee -a /etc/pacman.conf > /dev/null <<'EOF'

[haex]
SigLevel = Required DatabaseRequired
Server = https://arch.haex.space/$arch
EOF

# 3. Install
sudo pacman -Syu haex-vault
```

`$arch` is a pacman built-in placeholder — it expands to `x86_64` or
`aarch64` (Arch Linux ARM) at install time, so the same `pacman.conf`
entry works on both architectures.

Future updates: handled automatically by `sudo pacman -Syu`.

## AppImage (no repo, manual updates)

If you'd rather not add a third-party repo, download the latest
`.AppImage` from the
[GitHub Releases page](https://github.com/haex-space/haex-vault/releases/latest),
`chmod +x`, and run it directly. Updates require re-downloading the
newest AppImage.

## Verifying the signing key

The same long-lived signing key signs `.deb`, `.rpm`, and `.pkg.tar.zst`
packages — it does **not** rotate per release. The expected fingerprint:

```
92B1 6ADF 139D F0D5 BA0B  2C8A 7940 193A 39D0 D4EA
```

The public key is mirrored under every repo prefix; pulling and printing
it from any of them must produce the fingerprint above:

- [apt.haex.space/pubkey.gpg](https://apt.haex.space/pubkey.gpg)
- [rpm.haex.space/pubkey.gpg](https://rpm.haex.space/pubkey.gpg)
- [arch.haex.space/pubkey.gpg](https://arch.haex.space/pubkey.gpg)

```bash
curl -fsSL https://apt.haex.space/pubkey.gpg | gpg --show-keys
```

The three URLs serve byte-identical content; comparing two of them is a
basic cross-check on top of the fingerprint match.

## Removing the repository

```bash
# Debian/Ubuntu
sudo rm /etc/apt/sources.list.d/haex-vault.list
sudo rm /etc/apt/keyrings/haex-vault.asc

# Fedora/RHEL
sudo rm /etc/yum.repos.d/haex-vault.repo

# Arch — remove the [haex] block from /etc/pacman.conf, then drop the
# trusted key (replace KEYID with the fingerprint shown by `pacman-key --list-keys`):
sudo pacman-key --delete KEYID
```
