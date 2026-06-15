# earworm task runner — `just` lists recipes

default:
    @just --list

# Desktop app in dev mode (vite hot-reload + debug Rust host)
dev:
    cd apps/desktop && pnpm tauri dev

# Release build of everything (headless daemon + UI binary + .deb bundle)
# Daemon first: the .deb bundle's files-map pulls in target/release/earwormd.
build:
    cargo build -p server --release
    cd apps/desktop && pnpm tauri build

# Stage the release .deb into dist/ for distribution
package: build
    #!/usr/bin/env bash
    set -euo pipefail
    mkdir -p dist
    deb=$(ls -t target/release/bundle/deb/*.deb | head -1)
    cp "$deb" dist/
    echo "staged $(basename "$deb") -> dist/"

# Run the release desktop app (builds if missing)
run:
    @test -x target/release/earworm || just build
    target/release/earworm

# Run the headless daemon (release)
daemon:
    @test -x target/release/earwormd || cargo build -p server --release
    target/release/earwormd

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
    subprocess.run(["git", "tag", "-a", f"v{v}", "-m", f"earworm v{v}"], check=True)
    print(f"tagged v{v} — push with: git push origin main --follow-tags")

# All tests: cargo workspace + vitest
test:
    cargo test --workspace
    cd apps/desktop && pnpm vitest run

# Lint gate: clippy, rustfmt check, svelte-check, theme-color guardrail
lint:
    cargo clippy --workspace --all-targets -- -D warnings
    cargo fmt --check
    cd apps/desktop && pnpm exec svelte-check --tsconfig ./tsconfig.json
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
        s.connect(os.environ["XDG_RUNTIME_DIR"] + "/earworm.sock")
    except OSError:
        sys.exit("earworm is not running (no socket)")
    s.sendall(b'{{json}}' + b"\n")
    print(s.recv(1 << 20).decode(), end="")

# Remove build artifacts
clean:
    cargo clean
    rm -rf apps/desktop/dist apps/desktop/node_modules/.vite
