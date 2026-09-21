# CLAUDE.md

Guidance for Claude Code when working in this repository.

## What this is

Forever Addons: a desktop addon manager for World of Warcraft: Forever
(Tauri 2: Rust 2024 in `src-tauri/`, React 19 + TypeScript + Vite in `ui/`).
It discovers, downloads, installs, and manages addons from pluggable sources:
CurseForge (keyed Core API, `gameVersionTypeId=88568`), Wago Addons (external
API, self-serve token, `game_version=forever`), and GitHub releases of
user-tracked repositories.

## Always follow

- Modern Rust 2024 idioms; no new `mod.rs` files (`foo.rs` + `foo/bar.rs`).
- Make illegal states unrepresentable with domain types.
- No traits without real polymorphism; sources dispatch via the exhaustive
  `SourceId` match in `src/source.rs`.
- Preserve unknown external data as `Unknown`/`Unsupported` variants instead
  of dropping it.
- The gate for "done" is `just check`: `cargo fmt --check`,
  `cargo clippy -D warnings`, `cargo test`, `npm --prefix ui run build`.

## Dev

- `just dev` runs the full app; `just ui` runs the UI alone against the mock
  backend (`ui/src/mock.ts`) on http://127.0.0.1:6273.
- `ui/src/types.ts` hand-mirrors the serde shapes; a Rust-side change to a
  command payload needs matching edits in `types.ts`, `api.ts`, and `mock.ts`.
- `just live` smoke-tests the real services (`CURSEFORGE_API_KEY`,
  `WAGO_TOKEN` env vars for the keyed ones).
