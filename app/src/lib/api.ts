import { invoke } from "@tauri-apps/api/core";
import { workshopUrl } from "./format";

export type Mode = "zomboid" | "viewpoint";
export type ModRow = {
  kind: "mod";
  id: string;
  name: string;
  source: string;
  on: boolean;
  missing: boolean;
  requires: string[];
  incompatible: string[];
  load_after: string[];
  load_before: string[];
  only41: boolean;
  only42: boolean;
};
type SepRow = { kind: "sep"; name: string };
export type Row = ModRow | SepRow;

export type View = {
  profile: string;
  profiles: string[];
  /** Kind of the active profile. */
  mode: Mode;
  /** Profile name -> kind. */
  kinds: Record<string, Mode>;
  /** The library: what's installed, organised by the player (its order doesn't change what loads). */
  rows: Row[];
  /** The load order: enabled mod ids, as written to the game. */
  order: string[];
  adopted: boolean;
  /** First run: this load created the first profile. */
  created: boolean;
  zomboid: string;
  workshop: string | null;
  build: 41 | 42 | null;
};

export type ModDetails = {
  id: string;
  name: string;
  source: string;
  dir: string;
  info: Record<string, string>;
  ours: boolean;
  has_backup: boolean;
};

export type WorkshopItem = {
  id: string;
  title: string;
  preview_url: string;
  description: string;
  subscriptions: number;
  time_updated: number;
  file_size: number;
  tags: string[];
  num_children: number;
  /** Steam's 0-5 star rating and its vote count. */
  stars: number;
  votes: number;
  views: number;
  favorited: number;
  comments: number;
  time_created: number;
  /** The author's Steam id, and their name (browse results only). */
  creator: string;
  author: string;
  collection: boolean;
  have: boolean;
};

/** A Workshop browse request (see steam_workshop::Query). */
export type Query = {
  search: string;
  /** 0 anywhere, 1 title, 2 description. */
  scope: number;
  sort: string;
  /** Window for the "trend" sort, in days. */
  days: number;
  page: number;
  per_page: 30 | 50;
  required: string[];
  excluded: string[];
  collections: boolean;
  /** Only what depends on (or, with collections, contains) this item. */
  child: string;
  libraries: boolean;
  incompatible: boolean;
  created_from: number;
  created_to: number;
  updated_from: number;
  updated_to: number;
};
export const newQuery = (over: Partial<Query> = {}): Query => ({
  search: "", scope: 0, sort: "totaluniquesubscribers", days: 7, page: 1, per_page: 30, required: [], excluded: [],
  collections: false, child: "", libraries: false, incompatible: false, created_from: 0, created_to: 0, updated_from: 0, updated_to: 0, ...over,
});
export type WorkshopInfo = { item: WorkshopItem | null; screenshots: string[]; videos: string[]; required: WorkshopItem[]; members: WorkshopItem[] };
export type ChangeNote = { time: number; text: string };
export type Comments = { total: number; comments: { author: string; time: number; text: string }[] };
export type Browse = { total: number; items: WorkshopItem[] };
/** Autosort's sections in list order, and each mod's section. */
export type Categories = { order: string[]; of: Record<string, string> };
export type Candidate = Omit<WorkshopItem, "have"> & { confirmed: boolean; required_by_author: boolean; fits_build: boolean };

export type ZbStatus = {
  game_dir: string | null;
  have_mod: boolean;
  files_current: boolean;
  launcher_patched: boolean;
  enabled: boolean;
  required_by: string[];
};
export const zbReady = (s: ZbStatus | null) => !!s && s.have_mod && s.files_current && s.launcher_patched && s.enabled;

export type Shared = { name: string; kind: string; list: string; order: string[]; workshop: Record<string, string> };

/** A share code: "PZMM1:" + base64 of the JSON, so it pastes cleanly into chat. */
export function encodeShare(s: Shared): string {
  const bytes = new TextEncoder().encode(JSON.stringify(s));
  let bin = "";
  for (const b of bytes) bin += String.fromCharCode(b);
  return "PZMM1:" + btoa(bin);
}

/** Accepts a share code or the raw JSON. */
export function decodeShare(text: string): Shared {
  const t = text.trim();
  const json = t.startsWith("PZMM1:") ? new TextDecoder().decode(Uint8Array.from(atob(t.slice(6)), (c) => c.charCodeAt(0))) : t;
  const s = JSON.parse(json);
  if (typeof s?.name !== "string" || typeof s?.list !== "string") throw new Error("That isn't a PZMM profile code.");
  return { name: s.name, kind: s.kind ?? "", list: s.list, order: Array.isArray(s.order) ? s.order : [], workshop: s.workshop ?? {} };
}

export type ModErrors = { name: string; count: number; example: string; id: string | null };
export type Finding = { level: "error" | "warning"; subject: string; text: string };
export type Stats = { size: number; modified: number };
export type Overlap = { wins_over: string[]; loses_to: string[] };
export type Conflict = { other: string; files: number; wins: boolean; examples: string[] };
export type Save = { mode: string; name: string; mods: string[]; modified: number };
export type ServersAndSaves = { servers: string[]; server_dir: string; saves: Save[] };

export const api = {
  load: () => invoke<View>("load"),
  // the backend only stores order, on/off and separators
  save: (rows: Row[], order: string[]) =>
    invoke<View>("save", { rows: rows.map((r) => (r.kind === "mod" ? { kind: r.kind, id: r.id, on: r.on } : { kind: r.kind, name: r.name })), order }),
  install: (sources: string[]) => invoke<View>("install", { sources }),
  updateAll: () => invoke<View>("update_all"),
  remove: (ids: string[]) => invoke<View>("remove", { ids }),
  rollback: (id: string) => invoke<View>("rollback", { id }),
  profileNew: (name: string) => invoke<View>("profile_new", { name }),
  profileUse: (name: string) => invoke<View>("profile_use", { name }),
  profileDelete: (name: string) => invoke<View>("profile_delete", { name }),
  setBuild: (build: 41 | 42 | null) => invoke<View>("set_build", { build }),
  details: (id: string) => invoke<ModDetails | null>("details", { id }),
  poster: (id: string) => invoke<ArrayBuffer>("poster", { id }),
  browse: (query: Query) => invoke<Browse>("workshop_browse", { query }),
  workshopInfo: (id: string) => invoke<WorkshopInfo>("workshop_info", { id }),
  workshopChanges: (id: string, page: number) => invoke<ChangeNote[]>("workshop_changes", { id, page }),
  workshopComments: (creator: string, id: string, start: number) => invoke<Comments>("workshop_comments", { creator, id, start }),
  workshopAuthor: (creator: string, page: number) => invoke<Browse>("workshop_author", { creator, page }),
  zbStatus: () => invoke<ZbStatus>("zb_status"),
  zbPreview: (remove: boolean) => invoke<string[]>("zb_preview", { remove }),
  zbInstall: () => invoke<View>("zb_install"),
  zbUninstall: () => invoke<View>("zb_uninstall"),
  findRequirement: (modId: string, neededBy: string) =>
    invoke<Candidate[]>("find_requirement", { modId, neededBy }),
  installMissing: () => invoke<[View, string[]]>("install_missing"),
  checkUpdates: () => invoke<string[]>("check_updates"),
  updateMods: (ids: string[]) => invoke<View>("update_mods", { ids }),
  modStats: () => invoke<Record<string, Stats>>("mod_stats"),
  categories: () => invoke<Categories>("categories"),
  conflictSummary: () => invoke<Record<string, Overlap>>("conflict_summary"),
  conflicts: (id: string) => invoke<Conflict[]>("conflicts", { id }),
  serversAndSaves: () => invoke<ServersAndSaves>("servers_and_saves"),
  serverMaps: (name: string) => invoke<string[]>("server_maps", { name }),
  serverExport: (name: string, maps: string[]) => invoke<string[]>("server_export", { name, maps }),
  serverStart: (name: string) => invoke<string>("server_start", { name }),
  serverImport: (name: string) => invoke<View>("server_import", { name }),
  saveUse: (mode: string, name: string) => invoke<View>("save_use", { mode, name }),
  doctor: () => invoke<Finding[]>("doctor"),
  consoleErrors: () => invoke<ModErrors[]>("console_errors"),
  profileExport: () => invoke<Shared>("profile_export"),
  profileImport: (shared: Shared) => invoke<View>("profile_import", { shared }),
  modePreview: (mode: Mode) => invoke<string[]>("mode_preview", { mode }),
  modeSwitch: (mode: Mode) => invoke<View>("mode_switch", { mode }),
  // failures are logged by the backend (they reach the activity log), so callers needn't handle them
  openPath: (path: string) => invoke<void>("open_path", { path }).catch(() => {}),
  openUrl: (url: string) => invoke<void>("open_url", { url }).catch(() => {}),
};

/** Human label + page URL for a source string from the backend. */
export function describeSource(source: string): { label: string; url: string | null } {
  const [kind, rest = ""] = source.split(/:(.*)/s);
  switch (kind) {
    case "workshop":
      return { label: "Workshop", url: workshopUrl(rest) };
    case "steam":
      return { label: "Subscribed", url: workshopUrl(rest) };
    case "github":
      return { label: "GitHub", url: `https://github.com/${rest.split("@")[0]}` };
    case "local":
      return { label: "Local file", url: null };
    default:
      return { label: "Manual", url: null };
  }
}
