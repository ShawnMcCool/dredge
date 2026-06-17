# AUR packaging

Two packages cover Arch users:

- **`dredge-bin`** — the one-command path. Downloads the prebuilt release
  tarball (`just tarball` output, attached to the GitHub Release) and drops its
  `/usr` tree into place. No Rust/Node toolchain. This is what most people want:
  `yay -S dredge-bin`.
- **`dredge`** — the from-source path. `makedepends` the full toolchain, clones
  the tagged commit, runs `cargo build -p server --release` +
  `pnpm tauri build --no-bundle`, and installs the raw binaries. For people who
  prefer building locally or are on a non-x86_64 arch.

Both install the same layout: `dredge`, `dredged`, `dredge-analyze`, and
`dredge-enable-ml` on `PATH`, the analyze Python impls under
`/usr/lib/dredge/`, the desktop entry, and the icon.

## Publishing a release to the AUR

These PKGBUILDs are templates kept in-repo; the AUR holds its own git repos
(`ssh://aur@aur.archlinux.org/dredge-bin.git`, `.../dredge.git`).

1. Cut the release: `just release X.Y.Z` then push `--follow-tags`. CI builds
   the artifacts and publishes the GitHub Release.
2. Bump `pkgver` in both PKGBUILDs.
3. For `dredge-bin`, set `sha256sums` to the tarball's checksum from the
   release's `SHA256SUMS` (the source `dredge` package builds from the git tag,
   so it keeps `SKIP`).
4. Regenerate `.SRCINFO`: `makepkg --printsrcinfo > .SRCINFO` in each dir.
5. Commit + push each to its AUR remote.

> **Validation note:** a full `makepkg` of either package can only run once the
> referenced tag/release actually exists on GitHub — `dredge-bin` needs the
> release tarball, and `dredge` clones `#tag=vX.Y.Z`. Until then these are
> validated by `bash -n` + `makepkg --printsrcinfo`. The first real release is
> the end-to-end test.

A CI step can later auto-push `dredge-bin` on each release (e.g. an
AUR-deploy action keyed off the published tag); kept manual for now.
