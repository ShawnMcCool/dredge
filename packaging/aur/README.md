# AUR packaging

Two packages cover Arch users. The package and binary are both named `dredge`;
the prebuilt package is `dredge-looper-bin` because the AUR name `dredge-bin` is
taken by an unrelated project.

- **`dredge-looper-bin`** — the one-command path. Downloads the prebuilt release
  tarball (`just tarball` output, attached to the GitHub Release) and drops its
  `/usr` tree into place. No Rust/Node toolchain. This is what most people want:
  `yay -S dredge-looper-bin`. Provides/conflicts `dredge`.
- **`dredge`** — the from-source path. `makedepends` the full toolchain, clones
  the tagged commit, runs `cargo build -p server --release` +
  `pnpm tauri build --no-bundle`, and installs the raw binaries. For people who
  prefer building locally or are on a non-x86_64 arch.

Both install the same layout: `dredge`, `dredged`, `dredge-analyze`, and
`dredge-enable-ml` on `PATH`, the analyze Python impls under
`/usr/lib/dredge/`, the desktop entry, and the icon.

## Publishing a release to the AUR

These PKGBUILDs are templates kept in-repo; the AUR holds its own git repos
(`ssh://aur@aur.archlinux.org/dredge-looper-bin.git`, `.../dredge.git`).

**Automated.** The `aur` job in `.github/workflows/release.yml` publishes both
packages after each tagged release: it reads the tarball checksum from the
release `SHA256SUMS`, bumps `pkgver`, regenerates `.SRCINFO`, and pushes to the
AUR. It runs `scripts/ship aur --ci` in an Arch container and needs the
`AUR_SSH_PRIVATE_KEY` repo secret (a deploy key registered on the maintainer's
AUR account). Nothing to do per release beyond cutting the GitHub release.

**Manual / local** (fallback, or to also commit the bump back to the repo) —
`scripts/ship aur [<version>]` does the same on an Arch box with an unlocked AUR
SSH key, and commits the in-repo PKGBUILD/`.SRCINFO` bump. The underlying steps:

1. Cut the release: `scripts/ship release <level> --notes <file>`. CI builds the
   artifacts and publishes the GitHub Release.
2. Bump `pkgver` in both PKGBUILDs.
3. For `dredge-looper-bin`, set `sha256sums` to the tarball's checksum from the
   release's `SHA256SUMS` (the source `dredge` package builds from the git tag,
   so it keeps `SKIP`).
4. Regenerate `.SRCINFO`: `makepkg --printsrcinfo > .SRCINFO` in each dir.
5. Commit + push each to its AUR remote (`git push origin HEAD:master`).
