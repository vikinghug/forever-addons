# Forever Addons task runner. Requires the Rust toolchain (see
# rust-toolchain.toml), Node 20+, and `cargo install tauri-cli` for `dev`
# and `build`.

# List available recipes.
default:
    @just --list

# The whole app: `cargo tauri dev` starts the UI dev server itself, from the
# beforeDevCommand in src-tauri/tauri.conf.json.
dev:
    cargo tauri dev

# UI alone on http://127.0.0.1:6273, against the mock backend — no Rust, no
# client, no network. This is where the interface gets built.
ui:
    npm --prefix ui run dev

# Install the UI's node_modules (fresh clone).
install:
    npm --prefix ui install

# The checks that gate "done". The crate carries no clippy baseline, so this
# denies warnings — keep it that way.
check:
    cd src-tauri && cargo fmt --all -- --check
    cd src-tauri && cargo clippy --all-targets -- -D warnings
    cd src-tauri && cargo test
    npm --prefix ui run build

# Smoke-test the sources against the live services. Slow, needs the network,
# and CurseForge/Wago need CURSEFORGE_API_KEY / WAGO_TOKEN in the environment.
live:
    cd src-tauri && cargo test --test live_sources -- --ignored --nocapture

# Format the Rust sources in place.
fmt:
    cd src-tauri && cargo fmt --all

# Release bundle (deb/rpm/AppImage per tauri.conf.json).
build:
    cargo tauri build
