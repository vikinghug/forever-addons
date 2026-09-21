# Forever Addons

A desktop addon manager for **World of Warcraft: Forever**. Browse, install,
update, and cleanly uninstall addons against your local Forever client.

## Sources

| Source | What it needs |
| --- | --- |
| **CurseForge** | An API key (`x-api-key`) — CurseForge's API has no user login. Paste yours on the Sources screen. |
| **Wago Addons** | An access token, generated in your [Wago account settings](https://addons.wago.io/) — takes a minute. |
| **GitHub** | Nothing. Paste `owner/repo` (or a GitHub URL) and the app tracks its releases — handy for addons that ship on GitHub before the stores. |

Each source's catalog is pulled on demand and cached on disk; nothing polls in
the background. Addons whose authors only distribute through their own pages
stay visible in the catalog with a link, and are never fetched behind their
backs.

## How installs work

The app validates your client folder (a Battle.net product directory like
`World of Warcraft/_classic_beta_`) before it will touch anything, unpacks
addon zips into `Interface/AddOns`, and records exactly which folders each
addon owns. Updates replace only those folders; uninstalls delete only those
folders. Anything you unpacked by hand is listed as unmanaged and never
touched. TOC interface numbers are checked against Forever's band (`16001` for
beta 1.60.x), so a leftover retail addon is flagged instead of silently
failing to load.

## Building

With [devenv](https://devenv.sh) and direnv, `direnv allow` gives you the
whole toolchain (Rust, just, Node, the Tauri native deps, and the WebKitGTK
environment fixes) — or bring your own: Rust (see `rust-toolchain.toml`),
Node 20+, `just`, and the
[Tauri prerequisites](https://tauri.app/start/prerequisites/) for your OS.

```sh
just install   # npm install for the UI
just dev       # run the app (cargo tauri dev)
just ui        # UI alone against a mock backend, no client needed
just check     # fmt + clippy -D warnings + tests + UI build
just build     # release bundles (deb/rpm/AppImage)
```

Network smoke tests for the source parsers: `just live` (CurseForge and Wago
need `CURSEFORGE_API_KEY` / `WAGO_TOKEN` in the environment; they skip
politely without them).

## License

MIT — see [LICENSE](LICENSE).
