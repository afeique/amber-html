# Linux packaging

Channels for the `amber` CLI on Linux (Plans.md 14.6). The `.deb` flow is
validated locally; the rest are config that builds on Linux / by the user.

## `.deb` (Debian/Ubuntu) — validated

Config lives in `crates/amber-cli/Cargo.toml` (`[package.metadata.deb]`).

```sh
cargo install cargo-deb
cargo deb -p amber-cli           # -> target/debian/amber-cli_<ver>_<arch>.deb
sudo dpkg -i target/debian/amber-cli_*.deb
```

Build on Linux for a usable package (on macOS cargo-deb warns and packages the
host binary — fine for validating the metadata, not for shipping). `$auto`
shared-lib deps are resolved by `dpkg-shlibdeps` on Linux.

## `.rpm` (Fedora/RHEL/openSUSE)

Config in `crates/amber-cli/Cargo.toml` (`[package.metadata.generate-rpm]`).

```sh
cargo install cargo-generate-rpm
cargo build --release -p amber-cli
cargo generate-rpm -p crates/amber-cli   # -> target/generate-rpm/*.rpm
```

## AUR (Arch)

`packaging/aur/PKGBUILD` builds `amber` from the tagged source tarball. To
publish: fill `sha256sums` with the release tarball hash, then push to the AUR
as `amber-html` (`makepkg -si` to test locally on Arch).

## Nix

The repo root `flake.nix` exposes the CLI:

```sh
nix build                # ./result/bin/amber
nix run . -- https://example.com --markdown -o ./out
```

For nixpkgs upstreaming, adapt it into a `pkgs/by-name/am/amber-html/package.nix`.
