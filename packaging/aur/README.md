# AUR packaging

One package covers Arch users: **`dredge`**, built from source.

- **`dredge`** — `makedepends` the full toolchain (rust/node/pnpm/just/clang),
  clones the tagged commit, runs `cargo build -p server --release` +
  `pnpm tauri build --no-bundle`, and installs the raw binaries. Because it
  compiles on the user's machine, it links against the system's own libraries.

It installs `dredge`, `dredged`, `dredge-analyze`, and `dredge-enable-ml` on
`PATH`, the analyze Python impls under `/usr/lib/dredge/`, the desktop entry, and
the icon.

## Why there's no `-bin` (prebuilt) package

There used to be a `dredge-looper-bin` that unpacked the Ubuntu-built release
tarball. It was removed: a dynamically-linked Ubuntu binary only runs on Arch
while every soname matches, and they diverge — Ubuntu's rubberband 3.x links
`librubberband.so.2`, but Arch ships rubberband 4.x (`librubberband.so.3`), so
the prebuilt binary fails to load at launch. No `depends=` line fixes that (no
Arch package provides `.so.2`). Building from source sidesteps it entirely. The
`.deb` on the releases page remains the prebuilt path for Debian/Ubuntu, where
the sonames match.

## Publishing a release to the AUR

This PKGBUILD is a template kept in-repo; the AUR holds its own git repo
(`ssh://aur@aur.archlinux.org/dredge.git`).

**Automated.** The `aur` job in `.github/workflows/release.yml` publishes the
package after each tagged release: it bumps `pkgver`, regenerates `.SRCINFO`, and
pushes to the AUR. It runs `scripts/ship aur --ci` in an Arch container and needs
the `AUR_SSH_PRIVATE_KEY` repo secret (a deploy key registered on the maintainer's
AUR account). Nothing to do per release beyond cutting the GitHub release.

**Manual / local** (fallback, or to also commit the bump back to the repo) —
`scripts/ship aur [<version>]` does the same on an Arch box with an unlocked AUR
SSH key, and commits the in-repo PKGBUILD/`.SRCINFO` bump. The underlying steps:

1. Cut the release: `scripts/ship release <level> --notes <file>`.
2. Bump `pkgver` in the PKGBUILD (the package builds from the git tag, so
   `sha256sums` stays `SKIP`).
3. Regenerate `.SRCINFO`: `makepkg --printsrcinfo > .SRCINFO`.
4. Commit + push to the AUR remote (`git push origin HEAD:master`).
