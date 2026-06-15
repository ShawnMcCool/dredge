# earworm task runner — `just` lists recipes

default:
    @just --list

# Desktop app in dev mode (vite hot-reload + debug Rust host)
dev:
    cd apps/desktop && pnpm tauri dev

# Release build of everything (UI binary + headless daemon)
build:
    cd apps/desktop && pnpm tauri build
    cargo build -p server --release

# Run the release desktop app (builds if missing)
run:
    @test -x target/release/earworm || just build
    target/release/earworm

# Run the headless daemon (release)
daemon:
    @test -x target/release/earwormd || cargo build -p server --release
    target/release/earwormd

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
