# PZMM: Project Zomboid Mod Manager

A small, open-source mod manager for **Project Zomboid** (Build 41 and Build 42).

- Install mods from the **Steam Workshop** (ID or URL) or from **GitHub** (`owner/repo`, optionally `@tag` or `@branch`)
- See every mod you have: Workshop subscriptions and mods in `Zomboid/mods`
- Enable, disable and reorder mods (load order), using the same `mods/default.txt` the game reads
- Update everything you installed with one click

No Python, no account, no Steam login needed. Workshop downloads use Valve's [SteamCMD](https://developer.valvesoftware.com/wiki/SteamCMD) with an anonymous login. The app fetches SteamCMD itself the first time you install a Workshop mod.

## Download

Get `PZMM.exe` from the [latest release](../../releases/latest) and run it. Windows may show a SmartScreen warning because the exe isn't code-signed. Click **More info → Run anyway**, or run from source (below).

## Use

1. Paste a Workshop ID/URL or a GitHub repo into the box and click **Install**. To install several at once, separate them with spaces.
2. Double-click a mod (or use **Enable / Disable**) to switch it on or off. Use **Move Up / Down** to change the load order.
3. Start the game. Your mods will be active.

Examples of what you can paste:

```
2169435993
https://steamcommunity.com/sharedfiles/filedetails/?id=2169435993
someone/their-pz-mod
someone/their-pz-mod@v1.2.0
https://github.com/someone/their-pz-mod
```

Installed mods are copied to `Zomboid/mods/<mod id>`. To remove a Steam Workshop subscription, unsubscribe in Steam. **Remove** only deletes mods in `Zomboid/mods`.

**Build 41 or 42?** The app keeps whatever format your `default.txt` already uses. If the file is new, the app uses the Build 42 format. Tick **Build 41 format** if you play B41. Before each save, the previous file is kept as `default.txt.bak`.

## Command line (`pzmm`)

`pzmm` is a single-file program for Windows and Linux, with nothing to install, so it suits dedicated servers. Download `pzmm-windows-x64.exe` or `pzmm-linux-x64` from the [latest release](../../releases/latest).

```
pzmm add 2169435993 someone/their-pz-mod ./MyMod.zip
pzmm list
pzmm enable ModId
pzmm disable ModId
pzmm remove ModId
pzmm update
pzmm rollback ModId          # restore the version before the last update
pzmm profile new Server      # profiles: named mod setups
pzmm profile use Server
pzmm build 41                # force the B41 default.txt format
```

- **Profiles:** the active profile is written to `default.txt`. Changes you make in the game's own mod menu are picked up on the next run.
- **Only touches its own mods:** `pzmm remove` deletes only mods PZMM installed. Steam Workshop subscriptions and mods you installed by hand are left alone.
- **On Linux:** SteamCMD needs 32-bit libraries (`lib32gcc-s1` on Debian/Ubuntu).

| Variable | Default | Purpose |
|---|---|---|
| `PZ_HOME` | `~/Zomboid` | Your Zomboid user folder |
| `PZMM_DATA` | `%LOCALAPPDATA%\pzmm` or `~/.local/share/pzmm` | Profiles, backups, SteamCMD |
| `GITHUB_TOKEN` | none | Private repos or a higher GitHub API rate limit |

## Build from source

- **`pzmm`:** install [Rust](https://rustup.rs), then run `cargo build --release`. Run the tests with `cargo test`.
- **The current Windows app (`PZMM.exe`):** needs Python 3.10+ with Tkinter. Run it with `python gui.py`. The new desktop app (v0.3) replaces it.
- **Plan:** see [docs/FOUNDATION.md](docs/FOUNDATION.md).
- **Releases:** pushing a `v*` tag builds every release file and publishes the release.

## License

MIT
