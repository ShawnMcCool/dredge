# Campaign: Linux distribution

Make Earworm installable by other Linux users without hand-building from a repo
checkout. Brainstormed + scoped 2026-06-16. Work directly on `main`.

> **For agentic workers:** phases are dependency-ordered. Each phase has a
> verification gate and a commit. Phase 0 (hygiene) unblocks everything; Phases
> 1–3 build the artifact + release pipeline; Phases 4–6 are the user-facing
> install paths (AUR, ML helper, docs) and can land in any order once 1–3 are
> solid. Nothing here touches the audio engine or the domain model.

## What it is

Earworm is currently **build-from-source only** — `git clone` + `just build`,
with the binaries left in `target/release/` and a `.desktop` file hardcoded to
`/home/shawn/...`. The goal is **distro packages** for the two audiences who
asked: **Arch** (AUR) and **Debian/Ubuntu** (`.deb`). The core package stays
lean; the heavy optional ML features (analyze venv, demucs stems) remain
opt-in and self-bootstrapping, but get a one-command enable helper.

Chosen approach (**B — CI-driven releases**): a tag-push builds the `.deb` and a
portable binaries tarball inside a **clean Ubuntu 22.04 container** (predictable
glibc/webkit floor, not the maintainer's rolling Arch libs), attaches them to a
**GitHub Release**, and the AUR `-bin` package pulls that tarball. Flatpak was
rejected: its sandbox fights Earworm's signature features (tapping *other apps'*
PipeWire nodes, bootstrapping `uv` venvs, spawning `demucs`).

## Decisions (final)

- **Two audiences, two paths.** Arch → AUR (`earworm-bin` convenience +
  `earworm` source); Debian/Ubuntu → `.deb` from GitHub Releases. rpm / AppImage
  / Flatpak are **out for v1** but are each one entry in `bundle.targets` away.
- **Lean core, optional ML.** Packages declare only the four native runtime
  deps. `uv` / `python` / `demucs` are `optdepends` / `Suggests`; the new
  `earworm-enable-ml` helper replaces the README's copy-paste venv blocks.
- **Predictable Debian floor.** The `.deb` is built on **Ubuntu 22.04** in CI,
  not on the maintainer's machine — so it links against an old-enough glibc /
  webkit2gtk-4.1 to install on 22.04+ / Debian 12+.
- **Single-source the version.** `tauri.conf.json:version` is the canonical
  product version; a `just release X.Y.Z` recipe bumps it + tags `vX.Y.Z`.
- **No engine/domain changes.** The `earworm-analyze` PATH fallback and the
  `demucs`-on-PATH lookup already exist; packaging only changes *install layout*
  and *build/release plumbing*.

## Existing surfaces this rides (verified 2026-06-16)

| Need | Mechanism | Location |
|------|-----------|----------|
| Analyze script resolution | `<exe>/../../scripts/analyze` → `$EARWORM_ANALYZE` → **`earworm-analyze` on PATH** | `crates/server/src/analysis.rs:46-69` |
| Analyze sibling resolution | wrapper resolves its own dir via `readlink -f "$0"`, execs `$HERE/analyze_impl.py` | `scripts/analyze:13,24` |
| Demucs lookup | `binary: "demucs"`, `which`-style PATH scan | `crates/server/src/stems.rs:25-31,113` |
| Daemon build | `cargo build -p server --release` → `target/release/earwormd` | `justfile` (`build`/`daemon`) |
| Desktop build | `pnpm tauri build` bundles the Svelte UI into `earworm` | `justfile` (`build`) |
| Bundle config | `bundle.active:false`, icons listed | `apps/desktop/src-tauri/tauri.conf.json:33-41` |
| Product version / id | `version:"0.1.0"`, `identifier:"dev.shawn.earworm"` | `apps/desktop/src-tauri/tauri.conf.json:4,6` |
| Dev desktop file (hardcoded paths — to fix) | `Exec=/home/shawn/...`, `Icon=/home/shawn/...` | `earworm.desktop` |
| Icons | `icon.png` + sized PNGs/icns/ico | `apps/desktop/src-tauri/icons/` |
| Install dirs (runtime) | DB `~/.local/share/earworm/`, venvs there, socket `$XDG_RUNTIME_DIR` | `README.md` Paths table |

## Runtime dependency map (declare these, per ecosystem)

| Library | Arch (`depends`) | Debian (`Depends`) | Why |
|---------|------------------|--------------------|-----|
| Rubber Band ≥3 | `rubberband` | `librubberband2` | pitch-preserving stretch (FFI) |
| PipeWire | `pipewire` | `libpipewire-0.3-0` | all audio out + capture |
| WebKitGTK 4.1 | `webkit2gtk-4.1` | `libwebkit2gtk-4.1-0` | Tauri webview (auto-added by Tauri deb) |
| GTK 3 | `gtk3` | `libgtk-3-0` | webview host (auto-added by Tauri deb) |

ML optionals (`optdepends` / `Suggests`): `uv`, `python` (analyze + songformer),
`demucs` (stems). Audio **decode** is pure-Rust (symphonia) and SQLite is
bundled — no ffmpeg, no system sqlite to declare.

---

## Phase 0 — Packaging hygiene & relocatable layout

**Goal:** the app installs cleanly to system paths from *any* layout — no
hardcoded home paths, analyze wrapper findable on PATH, one version source.

**Files:** `earworm.desktop` (+ a packaged variant), `justfile`, possibly a new
`packaging/` dir.

- [ ] **0.1** Replace the hardcoded `earworm.desktop` with a **relocatable**
  one: `Exec=earworm`, `Icon=earworm`, drop the absolute paths. Keep
  `StartupWMClass=earworm`, categories, keywords. This file is what source/AUR
  installs ship; the Tauri `.deb` generates its own from `tauri.conf.json`.
- [ ] **0.2** Define the **installed analyze layout**: the three wrapper files
  (`analyze`, `analyze_impl.py`, `songformer_impl.py`) install together into
  `/usr/lib/earworm/`, with `/usr/bin/earworm-analyze` a **symlink** to
  `/usr/lib/earworm/analyze`. The wrapper's `readlink -f "$0"` resolves the
  symlink back to `/usr/lib/earworm/`, so `$HERE/analyze_impl.py` is found
  unmodified (verified `scripts/analyze:13,24`). No Rust change — `analysis.rs`
  already falls back to `earworm-analyze` on PATH.
- [ ] **0.3** `just release X.Y.Z` recipe: bump `tauri.conf.json:version`, commit
  `chore(release): vX.Y.Z`, and create an annotated tag `vX.Y.Z`. Document that
  the tag is what CI builds from.
- [ ] **Gate:** `desktop-file-validate earworm.desktop` clean (if available);
  `just lint` clean; manual read-through confirms zero `/home/shawn` strings in
  any shipped file (`grep -rn "/home/shawn" earworm.desktop packaging/ scripts/`).
  **Commit:** `chore(packaging): relocatable desktop entry + installed analyze layout`.

## Phase 1 — Tauri `.deb` bundling

**Goal:** `just build` (or a dedicated recipe) emits a `.deb` that declares the
right deps and installs the binary, daemon, desktop entry, and icons.

**Files:** `apps/desktop/src-tauri/tauri.conf.json`, `justfile`.

- [ ] **1.1** Flip `bundle.active: true` and set
  `bundle.targets: ["deb"]`. Add `bundle.linux.deb.depends:
  ["librubberband2", "libpipewire-0.3-0"]` (webkit2gtk/gtk are auto-added by the
  Tauri deb packager). Confirm the generated control file lists all four.
- [ ] **1.2** Decide `earwormd` packaging: the Tauri bundle only knows the
  `earworm` GUI binary, so ship the daemon via the `.deb`'s `bundle.linux.deb.files`
  map (or fold it into the AUR/tarball install scripts). Pick the files-map
  approach so one `.deb` carries both `earworm` and `earwormd`, plus
  `earworm-enable-ml` and the `/usr/lib/earworm/` analyze trio + symlink.
- [ ] **1.3** New `just package` recipe: `pnpm tauri build` → collect the `.deb`
  from `target/release/bundle/deb/` to a predictable `dist/` path.
- [ ] **Gate:** `just package` produces a `.deb`; `dpkg-deb -I` shows the four
  runtime deps and `dpkg-deb -c` shows `/usr/bin/earworm`, `/usr/bin/earwormd`,
  `/usr/bin/earworm-enable-ml`, `/usr/lib/earworm/analyze*`, the desktop entry,
  and icons. (Full install smoke-test happens in Phase 3 on real Ubuntu.)
  **Commit:** `feat(packaging): emit a Debian .deb with declared runtime deps`.

## Phase 2 — Release artifacts (tarball + checksums)

**Goal:** a portable binaries tarball + `SHA256SUMS` the AUR `-bin` package and
manual installers can consume, produced by a single recipe.

**Files:** `justfile`, `packaging/` (a small `install`/`layout` helper as needed).

- [ ] **2.1** `just tarball` recipe: assemble `earworm-vX.Y.Z-x86_64-linux.tar.gz`
  containing `earworm`, `earwormd`, `earworm-enable-ml`, the analyze trio,
  `earworm.desktop`, and the icon — laid out so the `-bin` PKGBUILD can drop them
  into `/usr/bin`, `/usr/lib/earworm`, `/usr/share/...` directly.
- [ ] **2.2** `just checksums` / fold into `just tarball`: emit `SHA256SUMS` over
  the `.deb` + tarball.
- [ ] **Gate:** `tar tzf` lists the expected tree; `sha256sum -c SHA256SUMS`
  passes against the built artifacts. **Commit:**
  `feat(packaging): portable binaries tarball + SHA256SUMS`.

## Phase 3 — CI (the core of Approach B)

**Goal:** PRs get a green-check gate; tag-push builds artifacts on a clean
Ubuntu and publishes a GitHub Release.

**Files:** `.github/workflows/ci.yml`, `.github/workflows/release.yml`.

- [ ] **3.1** `ci.yml` — on PR/push to `main`: on `ubuntu-22.04`, install native
  deps (`librubberband-dev libpipewire-0.3-dev libspa-0.2-dev
  libwebkit2gtk-4.1-dev libgtk-3-dev clang pkg-config build-essential`) + Rust +
  pnpm + `just`, run `just check` (test + lint). This is the contributor gate.
- [ ] **3.2** `release.yml` — on tag `v*`: same Ubuntu 22.04 toolchain, run
  `just package` + `just tarball`, then **install-smoke-test** the `.deb`
  (`sudo apt install ./dist/*.deb` then `earworm --help`/version) to catch
  missing-dep regressions on the real target. Upload `.deb`, tarball,
  `SHA256SUMS` to a GitHub Release (`softprops/action-gh-release`).
- [ ] **3.3** Cache cargo + pnpm + the Rust target dir to keep release builds
  reasonable; pin action versions.
- [ ] **Gate:** open a throwaway PR → `ci.yml` green; push a pre-release tag
  (e.g. `v0.1.0-rc1`) → `release.yml` builds, the `.deb` install-smoke-test
  passes on Ubuntu, and the Release carries all three artifacts.
  **Commit:** `ci: PR check gate + tag-driven release build (Ubuntu 22.04)`.

## Phase 4 — AUR packaging

**Goal:** Arch users install via the AUR — `-bin` for the one-command path,
source for the from-scratch path.

**Files:** `packaging/aur/earworm-bin/PKGBUILD`,
`packaging/aur/earworm/PKGBUILD` (+ `.SRCINFO` for each).

- [ ] **4.1** `earworm-bin/PKGBUILD` — `source` = the GitHub Release tarball for
  the tagged version; `sha256sums` from the release; `depends`: rubberband
  pipewire webkit2gtk-4.1 gtk3; `optdepends`: uv, python, demucs; `package()`
  drops the tarball tree into place (`/usr/bin/{earworm,earwormd,earworm-enable-ml}`,
  `/usr/lib/earworm/`, desktop entry, icon). No toolchain. This is `yay -S earworm-bin`.
- [ ] **4.2** `earworm/PKGBUILD` (source) — `makedepends`: rust nodejs pnpm just
  clang pkgconf; same `depends`/`optdepends`; `build()` runs the offline-ish
  `just build`; `package()` installs the raw binaries + relocatable desktop
  entry + analyze layout. Mirrors what the `.deb` ships, built locally.
- [ ] **4.3** Generate `.SRCINFO` for both; document the AUR publish step (manual
  `git push` to `aur.archlinux.org`, or the optional CI auto-publish in 3.x via
  an AUR-deploy action keyed off the release).
- [ ] **Gate:** `namcap PKGBUILD` clean-ish on both; `makepkg -f` in
  `packaging/aur/earworm-bin/` builds and `pacman -Qlp` on the result shows the
  expected file tree; the source `earworm` PKGBUILD `makepkg`-builds end to end
  on the maintainer's Arch box. **Commit:**
  `feat(packaging): AUR PKGBUILDs (earworm-bin + earworm source)`.

## Phase 5 — ML-enable helper

**Goal:** one command replaces the README's venv/uv/demucs copy-paste blocks.

**Files:** new `scripts/earworm-enable-ml` (extensionless, shebang —
diagnostics to stderr, per house script conventions).

- [ ] **5.1** Subcommands: `analyze` (bootstrap the analyze venv — really just
  trigger the wrapper's own first-run bootstrap, or pre-create it), `songformer`
  (the SongFormer venv from the README block), `stems`
  (`uv tool install demucs --with torchcodec`), and `all`. Idempotent — detect
  already-installed and skip. Honour `$EARWORM_*_VENV` overrides.
- [ ] **5.2** Friendly preflight: check `uv` is on PATH, print the one-liner to
  get it if not; note the GPU/disk expectations up front.
- [ ] **5.3** Ship it on PATH from all install paths (Phase 1 files-map, Phase 2
  tarball, Phase 4 PKGBUILDs) and list it in the packages' `optdepends`/docs.
- [ ] **Gate:** `shellcheck scripts/earworm-enable-ml` clean; running
  `earworm-enable-ml stems` installs demucs and a second run is a no-op; stdout
  stays quiet (diagnostics on stderr). **Commit:**
  `feat(scripts): earworm-enable-ml one-command ML setup`.

## Phase 6 — Docs + optional in-app affordance

**Goal:** the README leads with "pick your install path"; ML setup points at the
helper.

**Files:** `README.md`; optional `apps/desktop/src/components/AnalyzePrompt.svelte`.

- [ ] **6.1** Rewrite README **Installation** into tabs/sections: **Arch**
  (`yay -S earworm-bin`), **Debian/Ubuntu** (download the `.deb`,
  `sudo apt install ./earworm_*.deb`), **Build from source** (the existing
  flow, condensed), then **Enable ML** (`earworm-enable-ml all`). Add a short
  **runtime requirements** banner: Linux-only, PipeWire mandatory (no
  ALSA/Pulse fallback), Ubuntu 22.04+ / Debian 12+ for webkit2gtk-4.1.
- [ ] **6.2** Update the Paths/overrides table if any install path moved
  (analyze wrapper now at `/usr/lib/earworm/`, override still `$EARWORM_ANALYZE`).
- [ ] **6.3** *(Stretch, optional)* `AnalyzePrompt.svelte`: when PREPARE can't
  find `uv`/`demucs`, surface an inline hint — "run `earworm-enable-ml all`" —
  instead of a bare failure. Flagged, not required for v1.
- [ ] **Gate:** README renders correctly (links resolve, code blocks copy-paste
  cleanly); `just lint` clean if 6.3 is done. **Commit:**
  `docs: pick-your-distro install + ML enable helper`.

---

## Execution order & status

Phase 0 → 1 → 2 → 3 are a chain (each builds on the prior artifact); 4, 5, 6 can
land in any order once 3 is green, though 5 (the helper) should precede 6 (docs
that reference it). Commit per phase on `main`. `just check` is the floor gate;
packaging phases add their own artifact-inspection gates, and Phase 3 is where
the `.deb` gets a real install smoke-test on Ubuntu.

**STATUS (2026-06-16): Phases 0–6 implemented + committed on `main`; live
GitHub gates pending.** What's done and locally verified:

- **Phase 0** — relocatable `earworm.desktop`; `just release X.Y.Z`. ✓
- **Phase 1** — `.deb` bundling. Built + inspected (`ar`/`tar`): Depends
  `librubberband2, libpipewire-0.3-0, libwebkit2gtk-4.1-0, libgtk-3-0`,
  Recommends `uv, demucs`, ships `/usr/bin/{earworm,earwormd,earworm-analyze,
  earworm-enable-ml}` + `/usr/lib/earworm/*.py`. ✓
- **Phase 2** — `just tarball`/`checksums`/`artifacts`; tarball tree + `sha256sum
  -c` verified. ✓
- **Phase 3** — `ci.yml` + `release.yml` (Ubuntu 22.04, `.deb` install
  smoke-test, Release publish). YAML validated; **live run pends a push/tag.** ⏳
- **Phase 4** — AUR `earworm-bin` + `earworm` PKGBUILDs + `.SRCINFO`; `bash -n`
  + `--printsrcinfo` validated. **Full `makepkg` pends a published v0.1.0
  release.** ⏳
- **Phase 5** — `scripts/earworm-enable-ml` (shellcheck-clean, idempotent). ✓
- **Phase 6** — README rewritten to distro-first install + ML helper. ✓ (6.3
  in-app AnalyzePrompt hint left as the flagged stretch, not done.)

**Remaining (needs the maintainer / outward-facing):** push `main` to trigger
`ci.yml`; push a tag (`v0.1.0` or an `-rc` first) to trigger `release.yml` and
produce the first GitHub Release; then publish the two PKGBUILDs to the AUR
(set `earworm-bin` `sha256sums` from the release `SHA256SUMS`).

## Self-review notes

- Every brainstorm decision maps to a phase: hygiene/relocatable layout (0),
  `.deb` (1), release artifacts (2), CI/Approach-B (3), AUR (4), ML helper (5),
  docs (6).
- **Out of scope by design:** rpm / AppImage / Flatpak (each a later
  `bundle.targets` entry), bundling torch/demucs into packages (multi-GB,
  GPU-specific — stays opt-in self-bootstrap), and any audio-engine or
  domain-model change.
- **No engine/domain code changes:** packaging rides the existing
  `earworm-analyze`-on-PATH fallback (`analysis.rs:61`) and `demucs`-on-PATH
  lookup (`stems.rs:113`); the only code-ish edits are config (`tauri.conf.json`),
  the relocatable `.desktop`, `justfile` recipes, CI YAML, PKGBUILDs, and one
  shell helper.
- **Key risk:** the Debian dependency floor. Mitigated by building the `.deb` in
  CI (3.2) with an install smoke-test, not on the maintainer's Arch box — the
  failure mode Approach A would have hidden.
- **Build floor correction (2026-06-16, first CI run).** The plan assumed
  Ubuntu **22.04** for an old glibc/webkit floor, but CI caught the opposite
  constraint: `libspa-sys` 0.10 generates bindings against the *system* libspa
  headers, and 22.04's PipeWire (0.3.48) is too old — missing
  `spa_meta_region_is_valid` / `spa_video_info_raw.flags`, so the build fails
  with E0425/E0560. The libspa bindings dictate a **newer** floor than glibc
  would. Both workflows moved to **ubuntu-24.04** (PipeWire 1.0+); documented
  floor is now Ubuntu 24.04+ / Debian 13+. Exactly the regression Approach B's
  CI build was meant to surface.
