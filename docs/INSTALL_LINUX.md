# Installing Haex Vault on Linux

Haex Vault ships native packages for Debian/Ubuntu (`.deb`) and
Fedora/RHEL/openSUSE (`.rpm`) via our own package repositories. Once
configured, future updates arrive through your system's package manager —
`apt upgrade` / `dnf upgrade` — just like any other system package.

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

## AppImage (no repo, manual updates)

If you'd rather not add a third-party repo, download the latest
`.AppImage` from the
[GitHub Releases page](https://github.com/haex-space/haex-vault/releases/latest),
`chmod +x`, and run it directly. Updates require re-downloading the
newest AppImage.

## Verifying the signing key

The repo signing key fingerprint is published at
[apt.haex.space/pubkey.gpg](https://apt.haex.space/pubkey.gpg). You can
inspect it before importing:

```bash
curl -fsSL https://apt.haex.space/pubkey.gpg | gpg --show-keys
```

## Removing the repository

```bash
# Debian/Ubuntu
sudo rm /etc/apt/sources.list.d/haex-vault.list
sudo rm /etc/apt/keyrings/haex-vault.asc

# Fedora/RHEL
sudo rm /etc/yum.repos.d/haex-vault.repo
```
