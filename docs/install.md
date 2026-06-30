# Installing Haex Vault

## Arch Linux (pacman)

```bash
# 1) Import the repo's PGP key and locally sign it so pacman trusts it
curl -fsSL https://arch.haex.space/pubkey.gpg -o /tmp/haex-vault.gpg
KEYID=$(gpg --with-colons --show-keys /tmp/haex-vault.gpg | awk -F: '/^pub/{print $5}' | head -1)
sudo pacman-key --add /tmp/haex-vault.gpg
sudo pacman-key --lsign-key "$KEYID"
rm /tmp/haex-vault.gpg

# 2) Add the repo to /etc/pacman.conf
sudo tee -a /etc/pacman.conf <<'EOF'

[haex-vault]
SigLevel = Required DatabaseRequired
Server = https://arch.haex.space/$arch
EOF

# 3) Install
sudo pacman -Sy haex-vault
```

> `$arch` is a pacman built-in placeholder — the same `Server` line works on `x86_64` and `aarch64`.

## Debian / Ubuntu (apt)

_TODO — extract from existing release notes / sibling apt-repo docs._

## Fedora / RHEL (rpm/dnf)

_TODO — extract from existing release notes / sibling rpm-repo docs._

## AppImage / direct `.deb` / `.rpm` download

See the [GitHub Releases page](https://github.com/haex-space/haex-vault/releases) for portable artifacts.
