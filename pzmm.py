"""pzmm - Project Zomboid mod manager core + CLI (Steam Workshop + GitHub), stdlib only.

  python pzmm.py add 2169435993 https://steamcommunity.com/sharedfiles/filedetails/?id=2392709985
  python pzmm.py add owner/repo  owner/repo@v1.2  https://github.com/owner/repo
  python pzmm.py list | enable ID.. | disable ID.. | remove ID.. | update

Env: PZ_HOME (default ~/Zomboid), STEAMCMD_DIR, GITHUB_TOKEN (optional).
"""
import argparse, io, json, os, re, shutil, subprocess, sys, tarfile, tempfile, urllib.request, zipfile
from pathlib import Path

HOME = Path(os.environ.get("PZ_HOME", Path.home() / "Zomboid"))
MODS = HOME / "mods"
DEFAULT = MODS / "default.txt"
STATE = MODS / "pzmm.json"  # {mod_id: source}
DATA = Path(os.environ.get("LOCALAPPDATA", Path.home() / ".local" / "share")) / "pzmm"
STEAMCMD_DIR = Path(os.environ.get("STEAMCMD_DIR", DATA / "steamcmd"))
APP = "108600"
VERSION_DIR = re.compile(r"\d+(\.\d+)*|common")  # B42 layout: Mod/42/mod.info, Mod/common/
NO_WINDOW = getattr(subprocess, "CREATE_NO_WINDOW", 0)


def fetch(url):
    req = urllib.request.Request(url, headers={"User-Agent": "pzmm"})
    if "api.github.com" in url and os.environ.get("GITHUB_TOKEN"):
        req.add_header("Authorization", f"Bearer {os.environ['GITHUB_TOKEN']}")
    with urllib.request.urlopen(req) as r:
        return r.read()


def load_state():
    return json.loads(STATE.read_text()) if STATE.exists() else {}


def save_state(state):
    STATE.write_text(json.dumps(state, indent=2))


def read_info(path):
    lines = path.read_text(errors="replace").splitlines()
    return {k.strip(): v.strip() for k, _, v in (l.partition("=") for l in lines if "=" in l)}


def find_mods(root, *patterns):
    """{mod_dir: mod.info} for every mod under root, B41 or B42 layout."""
    found = {}
    for pat in patterns or ("**/",):
        for info in sorted(Path(root).glob(pat + "mod.info")):
            d = info.parent.parent if VERSION_DIR.fullmatch(info.parent.name) else info.parent
            found.setdefault(d, info)
    return found


def steam_workshop():
    """Folder where Steam keeps this user's PZ Workshop subscriptions, or None."""
    roots = [Path.home() / ".steam" / "steam", Path.home() / ".local" / "share" / "Steam"]
    if os.name == "nt":
        import winreg
        try:
            with winreg.OpenKey(winreg.HKEY_CURRENT_USER, r"Software\Valve\Steam") as k:
                roots.insert(0, Path(winreg.QueryValueEx(k, "SteamPath")[0]))
        except OSError:
            pass
    for root in roots:
        vdf = root / "steamapps" / "libraryfolders.vdf"
        libs = re.findall(r'"path"\s*"([^"]+)"', vdf.read_text(errors="replace")) if vdf.exists() else []
        for lib in [root, *(Path(l.replace("\\\\", "\\")) for l in libs)]:
            if (d := lib / "steamapps" / "workshop" / "content" / APP).is_dir():
                return d


def installed():
    """{id: (name, source, dir)}: Steam subscriptions, then Zomboid/mods (which wins on clashes)."""
    out, state = {}, load_state()
    if ws := steam_workshop():
        for d, info in find_mods(ws, "*/mods/*/", "*/mods/*/*/").items():
            i = read_info(info)
            out[i.get("id", d.name)] = (i.get("name", ""), f"steam:{d.relative_to(ws).parts[0]}", d)
    if MODS.exists():
        for d, info in find_mods(MODS, "*/", "*/*/").items():
            i = read_info(info)
            mid = i.get("id", d.name)
            out[mid] = (i.get("name", ""), state.get(mid, "manual"), d)
    return out


def install(root, source):
    mods = find_mods(root)
    if not mods:
        print(f"{source}: no mod.info found", file=sys.stderr)
        return []
    MODS.mkdir(parents=True, exist_ok=True)
    state, ids = load_state(), []
    for d, info in mods.items():
        mid = read_info(info).get("id", d.name)
        dest = MODS / re.sub(r'[<>:"/\\|?*]', "_", mid)
        if dest.exists():
            shutil.rmtree(dest)
        shutil.copytree(d, dest)
        state[mid] = source
        ids.append(mid)
        print(f"installed {mid} ({source})")
    save_state(state)
    return ids


def steamcmd():
    exe = STEAMCMD_DIR / ("steamcmd.exe" if os.name == "nt" else "steamcmd.sh")
    if not exe.exists():
        print("downloading steamcmd...")
        STEAMCMD_DIR.mkdir(parents=True, exist_ok=True)
        base = "https://steamcdn-a.akamaihd.net/client/installer/"
        if os.name == "nt":
            zipfile.ZipFile(io.BytesIO(fetch(base + "steamcmd.zip"))).extractall(STEAMCMD_DIR)
        else:
            tarfile.open(fileobj=io.BytesIO(fetch(base + "steamcmd_linux.tar.gz"))).extractall(STEAMCMD_DIR, filter="data")
    return exe


def add_workshop(ids):
    if not ids:
        return []
    cmd = [str(steamcmd()), "+login", "anonymous"]
    for i in ids:
        cmd += ["+workshop_download_item", APP, i, "validate"]
    print(f"downloading {len(ids)} workshop item(s)...")
    with subprocess.Popen(cmd + ["+quit"], stdout=subprocess.PIPE, stderr=subprocess.STDOUT, stdin=subprocess.DEVNULL,
                          text=True, errors="replace", creationflags=NO_WINDOW) as p:
        for line in p.stdout or ():
            if "workshop" in line.lower() or "error" in line.lower():
                print(line.rstrip())
    out = []
    for i in ids:  # steamcmd's exit code is unreliable; check the folders instead
        root = STEAMCMD_DIR / "steamapps" / "workshop" / "content" / APP / i
        if root.exists():
            out += install(root, f"workshop:{i}")
        else:
            print(f"workshop:{i}: download failed", file=sys.stderr)
    return out


def add_github(spec):
    repo, _, ref = spec.partition("@")
    if not re.fullmatch(r"[\w.-]+/[\w.-]+", repo):
        print(f"bad github repo: {spec}", file=sys.stderr)
        return []
    url = f"https://api.github.com/repos/{repo}/zipball" + (f"/{ref}" if ref else "")
    print(f"downloading github:{spec}...")
    with tempfile.TemporaryDirectory() as tmp:
        zipfile.ZipFile(io.BytesIO(fetch(url))).extractall(tmp)  # extractall strips ../ and absolute paths
        return install(tmp, f"github:{spec}")


def parse_source(s):
    """-> ('workshop', id) | ('github', 'owner/repo[@ref]')"""
    if m := re.fullmatch(r"\d+|.*steamcommunity\.com/.*[?&]id=(\d+).*", s):
        return "workshop", m.group(1) or s
    s = re.sub(r"^(https?://)?github\.com/", "", s).removesuffix(".git").strip("/")
    return "github", s


def add(sources):
    parsed = [parse_source(s) for s in sources]
    ids = add_workshop([v for k, v in parsed if k == "workshop"])
    for k, v in parsed:
        if k == "github":
            try:
                ids += add_github(v)
            except OSError as e:  # URLError/HTTPError: bad repo, ref, or network
                print(f"github:{v}: {e}", file=sys.stderr)
    return ids


def update():
    return add(list(dict.fromkeys(src.split(":", 1)[1] for src in load_state().values())))


def remove(ids):
    state = load_state()
    for i in ids:
        if i in state or (MODS / i / "mod.info").exists() or any((MODS / i).glob("*/mod.info")):
            shutil.rmtree(MODS / i, ignore_errors=True)
            state.pop(i, None)
        else:
            print(f"{i}: not in Zomboid/mods (unsubscribe Steam mods in Steam)", file=sys.stderr)
    save_state(state)


# --- default.txt: enabled mods in load order ---

def read_enabled():
    """-> (ids, maps_block_body, is_b41)"""
    t = DEFAULT.read_text() if DEFAULT.exists() else ""
    mods = re.search(r"\bmods\s*\{(.*?)\}", t, re.S)
    maps = re.search(r"\bmaps\s*\{(.*?)\}", t, re.S)
    ids = re.findall(r"mod\s*=\s*\\?([^,\r\n]+?)\s*,", mods.group(1)) if mods else []
    return ids, maps.group(1) if maps else "\n", bool(ids) and not re.search(r"mod\s*=\s*\\", t)


def write_enabled(ids, b41=None):
    """b41=None keeps the file's current format (B42 `mod = \\Id,` if unknown)."""
    _, maps, was_b41 = read_enabled()
    prefix = "" if (was_b41 if b41 is None else b41) else "\\"
    MODS.mkdir(parents=True, exist_ok=True)
    if DEFAULT.exists():
        shutil.copy(DEFAULT, DEFAULT.with_suffix(".txt.bak"))
    body = "".join(f"    mod = {prefix}{i},\n" for i in dict.fromkeys(ids))
    DEFAULT.write_text(f"VERSION = 1,\n\nmods\n{{\n{body}}}\n\nmaps\n{{{maps}}}\n")


def main(argv=None):
    p = argparse.ArgumentParser(description="Project Zomboid mod manager")
    p.add_argument("--b41", action="store_true", default=None, help="write default.txt in Build 41 format")
    sub = p.add_subparsers(dest="cmd", required=True)
    a = sub.add_parser("add", help="install from workshop id/url or github owner/repo[@ref]")
    a.add_argument("sources", nargs="+")
    a.add_argument("--no-enable", action="store_true")
    for name in ("enable", "disable", "remove"):
        sub.add_parser(name).add_argument("ids", nargs="+")
    sub.add_parser("list")
    sub.add_parser("update", help="re-download everything pzmm installed")
    args = p.parse_args(argv)
    enabled = read_enabled()[0]

    if args.cmd == "add":
        ids = add(args.sources)
        if ids and not args.no_enable:
            write_enabled(enabled + ids, args.b41)
    elif args.cmd == "enable":
        write_enabled(enabled + args.ids, args.b41)
    elif args.cmd == "disable":
        write_enabled([i for i in enabled if i not in args.ids], args.b41)
    elif args.cmd == "remove":
        remove(args.ids)
        write_enabled([i for i in enabled if i not in args.ids], args.b41)
    elif args.cmd == "update":
        update()
    elif args.cmd == "list":
        mods = installed()
        for i, (name, src, _) in sorted(mods.items()):
            print(f"[{'x' if i in enabled else ' '}] {i:30} {name:40} {src}")
        for i in enabled:
            if i not in mods:
                print(f"[x] {i:30} {'(missing)':40}")


if __name__ == "__main__":
    main()
