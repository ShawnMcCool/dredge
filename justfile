# dredge task runner — `just` lists recipes

default:
    @just --list

# Desktop app in dev mode (vite hot-reload + debug Rust host)
dev:
    cd apps/desktop && pnpm tauri dev

# Release build of everything (headless daemon + UI binary + .deb bundle)
# Daemon first: the .deb bundle's files-map pulls in target/release/dredged.
build:
    cargo build -p server --release
    cd apps/desktop && pnpm tauri build

# Build everything fresh, then launch the release desktop app
go: build
    target/release/dredge

# Stage the release .deb into dist/ for distribution
package: build
    #!/usr/bin/env bash
    set -euo pipefail
    mkdir -p dist
    deb=$(ls -t target/release/bundle/deb/*.deb | head -1)
    cp "$deb" dist/
    echo "staged $(basename "$deb") -> dist/"

# Portable binaries tarball into dist/ (relocatable /usr tree the AUR -bin
# package and manual installs consume)
tarball: build
    #!/usr/bin/env bash
    set -euo pipefail
    ver=$(python3 -c "import json;print(json.load(open('apps/desktop/src-tauri/tauri.conf.json'))['version'])")
    name="dredge-${ver}-x86_64-linux"
    stage="dist/${name}"
    rm -rf "$stage"
    install -Dm755 target/release/dredge                       "$stage/usr/bin/dredge"
    install -Dm755 target/release/dredged                      "$stage/usr/bin/dredged"
    install -Dm755 scripts/analyze                              "$stage/usr/bin/dredge-analyze"
    install -Dm755 scripts/dredge-enable-ml                    "$stage/usr/bin/dredge-enable-ml"
    install -Dm755 scripts/dredge-doctor                       "$stage/usr/bin/dredge-doctor"
    install -Dm644 scripts/analyze_impl.py                      "$stage/usr/lib/dredge/analyze_impl.py"
    install -Dm644 scripts/songformer_impl.py                   "$stage/usr/lib/dredge/songformer_impl.py"
    install -Dm644 dredge.desktop                              "$stage/usr/share/applications/dredge.desktop"
    install -Dm644 apps/desktop/src-tauri/icons/128x128@2x.png  "$stage/usr/share/icons/hicolor/256x256/apps/dredge.png"
    tar -C dist -czf "dist/${name}.tar.gz" "$name"
    rm -rf "$stage"
    echo "built dist/${name}.tar.gz"

# SHA256SUMS over everything staged in dist/ (.deb + .tar.gz)
checksums:
    #!/usr/bin/env bash
    set -euo pipefail
    cd dist
    sha256sum *.deb *.tar.gz > SHA256SUMS
    cat SHA256SUMS

# Full release artifact set into dist/: .deb + tarball + SHA256SUMS (CI uses this)
artifacts: package tarball checksums

# Run the release desktop app (builds if missing)
run:
    @test -x target/release/dredge || just build
    target/release/dredge

# Run the headless daemon (release)
daemon:
    @test -x target/release/dredged || cargo build -p server --release
    target/release/dredged

# Tail the backend log (the desktop app funnels stdout/stderr here when launched
# without a terminal). Set DREDGE_DEBUG=1 before launching the app for the
# verbose timing/recording lines.
logs:
    @path="${XDG_DATA_HOME:-$HOME/.local/share}/dredge/dredge.log"; \
        echo "tailing $path"; touch "$path"; tail -n 100 -f "$path"

# Cut a release: bump the canonical version + tag (CI builds the artifacts), e.g.:
#   just release 0.2.0
release version:
    #!/usr/bin/env python3
    import json, re, subprocess, sys
    v = "{{version}}"
    if not re.fullmatch(r"\d+\.\d+\.\d+(-[0-9A-Za-z.-]+)?", v):
        sys.exit(f"not a semver: {v} (want MAJOR.MINOR.PATCH[-pre])")
    if subprocess.run(["git", "diff", "--quiet"]).returncode or \
       subprocess.run(["git", "diff", "--cached", "--quiet"]).returncode:
        sys.exit("working tree not clean — commit or stash first")
    conf = "apps/desktop/src-tauri/tauri.conf.json"
    with open(conf) as f:
        data = json.load(f)
    data["version"] = v
    with open(conf, "w") as f:
        json.dump(data, f, indent=2)
        f.write("\n")
    subprocess.run(["git", "add", conf], check=True)
    subprocess.run(["git", "commit", "-m", f"chore(release): v{v}"], check=True)
    subprocess.run(["git", "tag", "-a", f"v{v}", "-m", f"dredge v{v}"], check=True)
    print(f"tagged v{v} — push with: git push origin main --follow-tags")

# All tests: cargo workspace + vitest
test:
    cargo test --workspace
    cd apps/desktop && pnpm vitest run

# Lint gate: clippy, rustfmt check, svelte-check, theme-color guardrail
lint:
    cargo clippy --workspace --all-targets -- -D warnings
    cargo fmt --check
    cd apps/desktop && pnpm exec svelte-check --tsconfig ./tsconfig.app.json
    ./scripts/check-theme-colors

# Format rust code
fmt:
    cargo fmt

# Full pre-commit gate
check: test lint

# Send a raw JSON command to a running instance, e.g.:
#   just cmd '{"id":1,"cmd":"song.list"}'
cmd json:
    #!/usr/bin/env python3
    import os, socket, sys
    s = socket.socket(socket.AF_UNIX)
    try:
        s.connect(os.environ["XDG_RUNTIME_DIR"] + "/dredge.sock")
    except OSError:
        sys.exit("dredge is not running (no socket)")
    s.sendall(b'{{json}}' + b"\n")
    print(s.recv(1 << 20).decode(), end="")

# Remove build artifacts
clean:
    cargo clean
    rm -rf apps/desktop/dist apps/desktop/node_modules/.vite
