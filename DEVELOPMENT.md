# Development

How to build dredge from source and work on it. Architecture notes and the
full conventions live in [`CLAUDE.md`](CLAUDE.md).

## Prerequisites

Native libraries plus the toolchain.

**Debian / Ubuntu**

```bash
sudo apt install librubberband-dev libpipewire-0.3-dev libspa-0.2-dev \
  libwebkit2gtk-4.1-dev libgtk-3-dev clang pkg-config build-essential
```

**Arch**

```bash
sudo pacman -S rubberband pipewire webkit2gtk-4.1 gtk3 clang pkgconf base-devel
```

Plus [rustup](https://rustup.rs), Node + [pnpm](https://pnpm.io) (not npm), and
[just](https://github.com/casey/just).

| Dependency | Why it's needed |
|------------|-----------------|
| `rubberband` (≥3.0) | pitch-preserving time-stretch (Rubber Band R3), FFI-linked by the engine |
| `pipewire` | audio output, plus the tuner's microphone input |
| `webkit2gtk-4.1`, `gtk3` | the Tauri webview that renders the UI (desktop app only) |
| `clang` / libclang | bindgen builds the PipeWire/`libspa-sys` bindings — the build fails without it |
| `pkgconf`, `base-devel` | `pkg-config` + a C compiler/linker for the FFI crates |
| rust · node · pnpm · just | build the Rust workspace and the Svelte/Tauri frontend |

## Build

```bash
git clone https://github.com/ShawnMcCool/dredge.git && cd dredge
just build      # daemon -> target/release/dredged, then the desktop app +
                # .deb bundle -> target/release/{dredge, bundle/deb/}
```

`just build` compiles the daemon, then `pnpm tauri build` bundles the Svelte UI
into `dredge` and emits a `.deb`. `just package` stages the `.deb` into
`dist/`; `just artifacts` adds a portable tarball + `SHA256SUMS` (what CI ships).

## Dev loop

```bash
just dev        # desktop app with vite hot-reload + a debug Rust host
just test       # cargo test --workspace + pnpm vitest run
just lint       # clippy (-D warnings), cargo fmt --check, svelte-check
just check      # the pre-commit gate: test + lint
just fmt        # cargo fmt
just            # list every recipe
```

Targeted runs:

```bash
cargo test -p engine                                  # one crate
cargo test -p practice store::tests::name_of_test     # one Rust test
cd apps/desktop && pnpm vitest run lib/waveform-math.test.ts   # one frontend test
```

**Heads-up: dev builds decode audio ~100× slower than release.** The workspace
`Cargo.toml` keeps dependencies (including the audio `engine`) at `opt-level = 2`
while leaving our own crates at `0` for fast incremental rebuilds — that one line
is what makes `just dev` usable on real songs. Don't remove it.

## Releasing

`scripts/ship` owns the release path — run it with no args for usage:

```bash
scripts/ship prepare <major|minor|patch>   # read-only: version + commits since the last tag
scripts/ship check                         # release-safety gate (schema + a full artifact build)
scripts/ship release <level> --notes FILE  # changelog, version bump, commit, tag, push
scripts/ship verify [version]              # poll the GitHub release for its artifacts
```

The version of record is `apps/desktop/src-tauri/tauri.conf.json`; a pushed `v*`
tag triggers `.github/workflows/release.yml`, which runs `just artifacts` and
publishes the `.deb` + tarball + `SHA256SUMS`.
