// Check setup's one rulebook: how bad each finding is, and what fixes it. The Home pill, the Check
// setup window and the launch check all read it, so they never disagree. Every key doctor.rs (and
// javamods.rs) can report is here; one missing falls back to "check" with Hide.
import type { Finding } from "./api";
import type { IconName } from "./Icon.svelte";

/** broken: the game won't load it or start; risk: code that could harm you; check: may misbehave; low: worth knowing. */
export type Severity = "broken" | "risk" | "check" | "low";
export const SEVERITIES: Severity[] = ["broken", "risk", "check", "low"];

const RISK = new Set([
  "javaShell", "javaEndpoint", "javaPrivate", "javaNativeFile", "javaBadClass", "modCodeFile", "javaEnvOptions",
  "luaEndpoint", "luaShell", "luaPrivate", "luaProcess", "luaDownload", "luaFileEscape",
  "launcherAgent", "launcherBootPath", "launcherClasspath", "gameJar", "gameClass",
]);
/** Common in legit mods (Viewpoint starts processes and names agent options), or the author's business. */
const LOW = new Set(["javaProcess", "javaLaunchFlags", "javaLoadsCode", "noName", "idMismatch", "workshopUnlisted", "notYet", "itemUnused", "requireMissing"]);
/** Warnings that stop the mod (or its Java part) working all the same. */
const BROKEN = new Set(["only41", "only42", "javaBlocked", "javaNoLeaf", "javaLeafOff"]);

export function severity(f: Finding): Severity {
  if (f.level === "error" || BROKEN.has(f.key)) return "broken";
  if (RISK.has(f.key)) return "risk";
  if (LOW.has(f.key) || f.level === "note") return "low";
  return "check";
}

/** A finding about a server's config (Servers fixes those), not what Play starts. */
export const isServer = (f: Finding) => f.subject.endsWith(".ini");

/** What the remedies can do: the page's own actions. Each resolves when done. */
export type Fixes = {
  /** Mod ids that are mods here (installed or listed): subjects that aren't (files, folders) get no mod actions. */
  isMod: (id: string) => boolean;
  /** The game can load this mod on the build played (a B41-only mod can't on B42). */
  canLoad: (id: string) => boolean;
  /** The Workshop item a mod is loaded from ("" for none). */
  itemOf: (id: string) => string;
  install: (id: string, by: string) => Promise<unknown>;
  setMods: (ids: string[], on: boolean) => Promise<unknown>;
  uninstall: (ids: string[]) => Promise<unknown>;
  sort: () => Promise<unknown>;
  trust: (sha: string) => Promise<unknown>;
  setBuild: (build: 41 | 42) => Promise<unknown>;
  zbSetup: () => Promise<unknown>;
  zbRemove: () => Promise<unknown>;
  update: (ids: string[]) => Promise<unknown>;
  findB42: (id: string) => Promise<unknown>;
  unblock: (sha: string) => Promise<unknown>;
  folderAuto: (key: string) => Promise<unknown>;
  installItem: (item: string) => Promise<unknown>;
  unsubscribe: (items: string[]) => Promise<unknown>;
  go: (tab: "zb" | "maps" | "settings" | "servers" | "authoring") => void;
  openUrl: (url: string) => void;
  launch: () => Promise<unknown>;
};

/** A fix: `label` is a check.fix.<label> key; `safe` ones run in Fix all (they never trust files, turn mods off or switch build). */
export type Remedy = { label: string; vars?: Record<string, string>; icon: IconName; run: () => Promise<unknown> | void; safe?: boolean };

const STEAM_VERIFY = "steam://validate/108600";
const STEAM_GAME = "steam://nav/games/details/108600";

/** The fixes for a group of the same finding (one key, same details) on mods `ids`, best first. */
export function remedies(f: Finding, ids: string[], x: Fixes): Remedy[] {
  const v = f.vars;
  const mods = ids.filter(x.isMod);
  const off: Remedy[] = mods.length ? [{ label: "turnOff", icon: "x", run: () => x.setMods(mods, false) }] : [];
  const uninstall: Remedy[] = mods.length ? [{ label: "uninstall", icon: "trash", run: () => x.uninstall(mods) }] : [];
  const trust: Remedy[] = f.action?.dismiss ? [{ label: "trust", icon: "check", run: () => x.trust(f.action!.dismiss!) }] : [];
  const sort: Remedy = { label: "sort", icon: "sort", run: x.sort, safe: true };
  const zb: Remedy = { label: "zbSetup", icon: "wand", run: x.zbSetup, safe: true };
  switch (f.key) {
    case "notInstalled":
      return [{ label: "download", icon: "download", run: async () => {
        for (const id of ids) await x.install(id, id); // one at a time: each places its mods in the list
      }, safe: true }, ...off];
    case "needsMissing":
      return [{ label: "downloadReq", vars: { req: v.req }, icon: "download", run: () => x.install(v.req, ids[0]), safe: true }, ...off];
    case "needsDisabled":
    case "requireDisabled": {
      const other = v.req ?? v.other;
      // a requirement this build can't load is no fix: it would only come back as Broken
      if (!x.canLoad(other)) return [...off, ...uninstall];
      return [{ label: "turnOn", vars: { name: other }, icon: "check", run: () => x.setMods([other], true), safe: true }, ...off];
    }
    case "needsLater":
    case "loadAfter":
    case "loadBefore":
      return [sort, ...off];
    case "incompatible":
      return [{ label: "turnOffOther", vars: { name: v.other }, icon: "x", run: () => x.setMods([v.other], false) }, ...off];
    case "only41":
      return [...off, ...uninstall, ...mods.slice(0, 1).map((id): Remedy => ({ label: "findB42", icon: "search", run: () => x.findB42(id) }))];
    case "only42":
    case "viewpoint41":
      return [{ label: "switchBuild", vars: { build: "42" }, icon: "swap", run: () => x.setBuild(42) }, ...off];
    case "serverBuild":
      return [{ label: "switchBuild", vars: { build: v.theirs }, icon: "swap", run: () => x.setBuild(v.theirs === "41" ? 41 : 42) }];
    case "versionMin":
      return [{ label: "updateGame", icon: "steam", run: () => x.openUrl(STEAM_GAME) }, ...off];
    case "versionMax":
    case "luaSyntax":
      return [{ label: "updateMod", icon: "refresh", run: () => x.update(mods), safe: true }, ...off];
    case "javaNoJar":
    case "javaBadSignature":
      return [{ label: "updateMod", icon: "refresh", run: () => x.update(mods), safe: true }, ...off, ...uninstall];
    case "javaBlocked":
      return [...(v.sha ? [{ label: "allow", icon: "check" as IconName, run: () => x.unblock(v.sha) }] : []), ...off];
    case "javaNoLeaf":
    case "javaLeafOff":
      return [{ label: "openJava", icon: "settings", run: () => x.go("zb") }, ...off];
    case "zbAgent":
      return [zb, ...off];
    case "zbBroken":
    case "zbChanged":
      return [zb, { label: "zbRemove", icon: "trash", run: x.zbRemove }];
    case "twoCopies":
      return [{ label: "removeCopy", icon: "trash", run: () => x.uninstall(mods) }];
    case "sameId":
    {
      // every item but the one the mod is loaded from: that one stays
      const keep = x.itemOf(ids[0]);
      const others = String(v.items ?? "").split(/,\s*/).filter((i) => i && i !== keep);
      return keep && others.length ? [{ label: "unsubscribeOthers", icon: "x", run: () => x.unsubscribe(others) }] : [];
    }
    case "notOriginal":
      return [{ label: "getOriginal", icon: "download", run: () => x.installItem(v.item) }, ...uninstall];
    case "workshopGone":
    case "workshopBanned":
      return [...mods.slice(0, 1).map((id): Remedy => ({ label: "findReplacement", icon: "search", run: () => x.findB42(id) })), ...off];
    case "mapFolderTwice":
    case "mapOverlap":
      return [{ label: "arrangeMaps", icon: "layers", run: () => x.go("maps") }];
    case "launcherAgent":
    case "launcherBootPath":
    case "launcherClasspath":
      return [...trust, { label: "verify", icon: "steam", run: () => x.openUrl(STEAM_VERIFY) }];
    case "gameJar":
    case "gameClass":
      return [...trust, { label: "verify", icon: "steam", run: () => x.openUrl(STEAM_VERIFY) }];
    case "missingSet":
    case "notPzSet":
    case "notPz":
      return [...(v.key ? [{ label: "autodetect", icon: "refresh" as IconName, run: () => x.folderAuto(v.key) }] : []), { label: "openSettings", icon: "settings", run: () => x.go("settings") }];
    case "missingEnv":
    case "notPzEnv":
    case "folderWarning":
      return [{ label: "openSettings", icon: "settings", run: () => x.go("settings") }];
    case "notYet":
      return [{ label: "startGame", icon: "play", run: x.launch }];
    case "noId":
    case "noName":
    case "idMismatch":
      return [{ label: "openAuthoring", icon: "pencil", run: () => x.go("authoring") }];
  }
  if (isServer(f)) return [{ label: "openServers", icon: "server", run: () => x.go("servers") }];
  // the Java and Lua scans: trust the file, or take the mod out
  if (trust.length) return [...trust, ...off, ...uninstall];
  return off;
}

/** Same finding on several mods (key and details alike): one line, one set of fixes. */
export type Group = { key: string; severity: Severity; finding: Finding; ids: string[]; hidden: boolean };

export function groups(found: Finding[]): Group[] {
  const by = new Map<string, Group>();
  for (const f of found) {
    const id = `${f.key}|${f.hidden}|${JSON.stringify(f.vars)}|${f.action?.dismiss ?? ""}`;
    const g = by.get(id);
    if (g) g.ids.push(f.subject);
    else by.set(id, { key: f.key, severity: severity(f), finding: f, ids: [f.subject], hidden: f.hidden });
  }
  return [...by.values()];
}

/** What the Home pill counts: what's wrong with what Play starts, less what the player hid. */
export const counted = (found: Finding[]) => found.filter((f) => !f.hidden && !isServer(f) && severity(f) !== "low");
