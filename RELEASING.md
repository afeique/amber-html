# Releasing & distribution

AmberHTML ships to **crates.io** (`amber-core`, `amber-cli`), **PyPI**
(`amber-html`, Python via UniFFI), **npm** (`amber-html`, Node via napi-rs),
**GHCR** (a Docker image), **GitHub Releases** (prebuilt CLI binaries), and
**Homebrew** (a tap). `.github/workflows/release.yml` does all of it when you
push a `vX.Y.Z` tag.

The pipeline is built and the configs are validated locally (crates package
via `cargo publish --dry-run`; the Python wheel and Node addon build + import;
workflow YAML is valid). What's left is **one-time account/secret setup** and
**pushing a tag** — those need credentials and run on CI, not in this repo.

---

## 1. One-time setup (you do this once)

Create the accounts/tokens, then add them under **GitHub → repo → Settings →
Secrets and variables → Actions**.

| Channel | Account / config | GitHub secret |
|---|---|---|
| **crates.io** | Log in at crates.io, **Account Settings → API Tokens → New Token** (scope: publish-new + publish-update). | `CARGO_REGISTRY_TOKEN` |
| **PyPI** | Create the `amber-html` project, then **PyPI → Trusted Publishing → Add**: owner `afeique`, repo `amber-html`, workflow `release.yml`, environment `pypi`. Also create a GitHub **Environment** named `pypi` (Settings → Environments). | *none* (OIDC) |
| **npm** | `npm login`, then create an **automation** access token (npmjs.com → Access Tokens). Confirm the name `amber-html` is free (`npm view amber-html`). | `NPM_TOKEN` |
| **GHCR** | Nothing — uses the built-in `GITHUB_TOKEN`. After the first push, make the package **public** (repo → Packages → amber-html → visibility). | *none* |
| **Homebrew** | Create a tap repo **`afeique/homebrew-amber`** and add `packaging/homebrew/amber.rb` to its `Formula/` dir. | *none* |

Check the names are available before the first release:
`cargo search amber-core`, `npm view amber-html`, and the PyPI project page.

## 2. Cut a release

1. **Bump the version** to `X.Y.Z` in two places (they must match the tag):
   - `Cargo.toml` → `[workspace.package] version` (drives crates.io, PyPI wheels, and the binaries)
   - `crates/amber-node/package.json` → `version` (drives npm)
2. Update `CHANGELOG.md` (move *Unreleased* into a dated `X.Y.Z` section).
3. Commit, then tag and push:
   ```sh
   git commit -am "release: vX.Y.Z"
   git tag vX.Y.Z
   git push origin master vX.Y.Z
   ```
   The tag triggers `release.yml`. Watch it under the **Actions** tab.

## 3. After the run

- **crates.io / PyPI / npm / GHCR / GitHub binaries** publish automatically.
- **Homebrew** (manual, one extra step): the source tarball's hash isn't known
  until the GitHub Release exists. Compute it and update the tap's formula:
  ```sh
  curl -sL https://github.com/afeique/amber-html/archive/refs/tags/vX.Y.Z.tar.gz | shasum -a 256
  # edit Formula/amber.rb in afeique/homebrew-amber: bump `url` + `sha256`, commit.
  ```

## 4. How users install (post-release)

```sh
cargo install amber-cli                       # Rust / crates.io
pipx install amber-html                        # Python (or: pip install amber-html)
npm install -g amber-html                      # Node
brew install afeique/amber/amber               # macOS/Linux (Homebrew tap)
docker run --rm ghcr.io/afeique/amber-html <url> --markdown -o /out
# or grab a prebuilt binary from the GitHub Release.
```

## Notes / gotchas

- **Publish order (crates.io):** `amber-core` must index before `amber-cli`
  resolves it; the workflow polls with `cargo publish --dry-run` before
  publishing the CLI. `amber-node` and `uniffi-bindgen` are `publish = false`.
- **Re-runs:** a version can be published only once per registry. To re-release,
  bump the patch version.
- **npm cross-prebuilds:** the `npm-build`/`npm-publish` jobs follow the standard
  `@napi-rs/cli` flow (`napi build --platform` per target, then
  `create-npm-dirs` + `artifacts` + `npm publish`). If you add/remove targets,
  keep the matrix and `package.json`'s `napi.targets` in sync.
- **Docker first run** downloads the pinned Chrome for Testing into
  `AMBER_CACHE_DIR` (`/var/cache/amber`); mount it as a volume to persist.
