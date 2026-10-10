"""Offline self-check: python test_pzmm.py"""
import os, tempfile
from pathlib import Path

tmp = Path(tempfile.mkdtemp())
os.environ["PZ_HOME"] = str(tmp / "Zomboid")
import pzmm

# fake workshop item: one B41 mod, one B42 mod
src = tmp / "ws" / "Contents" / "mods"
(src / "ModA").mkdir(parents=True)
(src / "ModA" / "mod.info").write_text("name=Mod A\nid=ModA\n")
(src / "ModB" / "42").mkdir(parents=True)
(src / "ModB" / "common").mkdir()
(src / "ModB" / "42" / "mod.info").write_text("name=Mod B\nid=ModB\n")
assert sorted(pzmm.install(tmp / "ws", "workshop:1")) == ["ModA", "ModB"]
assert (pzmm.MODS / "ModB" / "42" / "mod.info").exists() and (pzmm.MODS / "ModB" / "common").exists()

# github zipball with the mod at repo root lands under its id, not "owner-repo-sha"
gh = tmp / "gh" / "owner-repo-abc123" / "42"
gh.mkdir(parents=True)
(gh / "mod.info").write_text("id=RootMod\n")
assert pzmm.install(tmp / "gh", "github:owner/repo") == ["RootMod"]
assert pzmm.load_state() == {"ModA": "workshop:1", "ModB": "workshop:1", "RootMod": "github:owner/repo"}

# Steam subscriptions are listed; a local copy wins
pzmm.steam_workshop = lambda: tmp / "steam"
sub = tmp / "steam" / "555" / "mods" / "Sub" / "42"
sub.mkdir(parents=True)
(sub / "mod.info").write_text("name = Subbed\nid = SubMod\n")
mods = pzmm.installed()
assert mods["SubMod"][:2] == ("Subbed", "steam:555") and mods["ModB"][:2] == ("Mod B", "workshop:1")
pzmm.remove(["SubMod"])  # not ours to delete
assert sub.exists()

# default.txt: B42 format, keeps maps block, dedupes, round-trips
pzmm.DEFAULT.write_text("VERSION = 1,\n\nmods\n{\n}\n\nmaps\n{\n    map = Muldraugh, KY,\n}\n")
pzmm.main(["enable", "ModA", "ModB", "ModA"])
assert "mod = \\ModA," in pzmm.DEFAULT.read_text() and "map = Muldraugh, KY," in pzmm.DEFAULT.read_text()
assert pzmm.read_enabled()[0] == ["ModA", "ModB"]
pzmm.main(["remove", "ModA"])
assert pzmm.read_enabled()[0] == ["ModB"] and not (pzmm.MODS / "ModA").exists()

# B41 file stays B41
pzmm.DEFAULT.write_text("VERSION = 1,\nmods\n{\n    mod = Old,\n}\nmaps\n{\n}\n")
pzmm.main(["enable", "ModB"])
assert "mod = ModB," in pzmm.DEFAULT.read_text() and pzmm.read_enabled()[0] == ["Old", "ModB"]
pzmm.write_enabled(["Old"], b41=False)  # explicit choice overrides detection
assert "mod = \\Old," in pzmm.DEFAULT.read_text()

assert pzmm.parse_source("https://steamcommunity.com/sharedfiles/filedetails/?id=2169435993&searchtext=") == ("workshop", "2169435993")
assert pzmm.parse_source("2169435993") == ("workshop", "2169435993")
assert pzmm.parse_source("https://github.com/owner/repo.git") == ("github", "owner/repo")
assert pzmm.parse_source("owner/repo@v1") == ("github", "owner/repo@v1")
print("ok")
