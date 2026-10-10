# PZMM: Project Zomboid Mod Manager

A free, open-source mod manager for **Project Zomboid** (Build 41 and 42), for Windows and Linux.

- **Install** from the Steam Workshop (link or ID), from GitHub (`owner/repo`, optionally `@tag`) or from a local `.zip` or folder. You don't need to log in to Steam.
- **See everything:** Steam Workshop subscriptions and the mods in your `Zomboid/mods` folder, in one list with each mod's poster, author and description.
- **Load order:** drag to reorder, group mods under separators, and switch mods on and off. PZMM writes the same `mods/default.txt` the game reads.
- **Profiles:** keep separate mod setups ("Solo", "Friends server") and switch between them in one click. Changes you make in the game's own mod menu are picked up automatically.
- **Safe updates:** **Update all** keeps the previous version of each mod, and **Restore previous version** brings it back if an update breaks your save.
- **Ownership:** PZMM only deletes mods it installed. It never touches your Workshop subscriptions or mods you installed by hand.

## Download

From the [latest release](../../releases/latest):

| System | File |
|---|---|
| Windows | `PZMM_<version>_x64-setup.exe` |
| Linux | `PZMM_<version>_amd64.AppImage` (any distribution) or `.deb` (Debian/Ubuntu) |
| Servers (no desktop) | `pzmm-linux-x64` or `pzmm-windows-x64.exe`: the command-line version, a single file |

The Windows installer isn't code-signed yet, so Windows may show a SmartScreen warning. Click **More info → Run anyway**.

## Use

1. Paste a Workshop link, a GitHub repo or a zip path into the box at the top, then click **Install**. The mod is installed and enabled.
2. Tick or untick mods to enable or disable them. Drag rows to change the load order. With a row selected, you can also use the keyboard: **Space** toggles it and **Alt+↑/↓** moves it.
3. Use **Add separator** to group your list, and double-click a separator to rename it.
4. Click **Launch game**.

**Build 41 or 42?** PZMM keeps whatever format your `default.txt` already uses. If the file is new, PZMM uses the Build 42 format. To set it yourself, go to **Settings → Game build**. Before each save, the previous file is kept as `default.txt.bak`.

## Command line (`pzmm`)

```
pzmm add 2169435993 someone/their-pz-mod ./MyMod.zip
pzmm list
pzmm enable ModId
pzmm disable ModId
pzmm remove ModId
pzmm update
pzmm rollback ModId          # restore the version before the last update
pzmm profile new Server
pzmm profile use Server
pzmm build 41                # force the B41 default.txt format
```

The command line and the app share the same profiles and data.

On Linux, SteamCMD needs 32-bit libraries (`lib32gcc-s1` on Debian/Ubuntu).

| Variable | Default | Purpose |
|---|---|---|
| `PZ_HOME` | `~/Zomboid` | Your Zomboid user folder |
| `PZMM_DATA` | `%LOCALAPPDATA%\pzmm` or `~/.local/share/pzmm` | Profiles, backups, SteamCMD |
| `GITHUB_TOKEN` | none | Private repos or a higher GitHub API rate limit |

## Build from source

This needs [Rust](https://rustup.rs) and Node.js 22 or newer. On Linux it also needs the [Tauri system packages](https://tauri.app/start/prerequisites/).

```
cargo test                                  # core + CLI
cd app && npm ci && npx tauri dev           # desktop app, live reload
cd app && npx tauri build                   # installers in target/release/bundle
```

How the code is organised:

| Path | What it is |
|---|---|
| `crates/core` | Shared logic. Everything specific to Project Zomboid lives in `pz.rs`. |
| `crates/cli` | The `pzmm` command line |
| `app/` | The desktop app: Tauri 2 with Svelte 5 |

The full plan is in [docs/FOUNDATION.md](docs/FOUNDATION.md). Pushing a `v*` tag builds every download and publishes a release.

## License

MIT
