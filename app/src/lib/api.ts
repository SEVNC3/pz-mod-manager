import { invoke } from "@tauri-apps/api/core";

export type ModRow = { kind: "mod"; id: string; name: string; source: string; on: boolean; missing: boolean; requires: string[] };
export type SepRow = { kind: "sep"; name: string };
export type Row = ModRow | SepRow;

export type View = {
  profile: string;
  profiles: string[];
  rows: Row[];
  adopted: boolean;
  zomboid: string;
  workshop: string | null;
  build: 41 | 42 | null;
};

export type Details = {
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
  have: boolean;
};
export type Browse = { total: number; items: WorkshopItem[] };
export type Candidate = Omit<WorkshopItem, "have"> & { confirmed: boolean; required_by_author: boolean };

export type ZbStatus = {
  game_dir: string | null;
  have_mod: boolean;
  files_current: boolean;
  launcher_patched: boolean;
  enabled: boolean;
  required_by: string[];
};
export const zbReady = (s: ZbStatus | null) => !!s && s.have_mod && s.files_current && s.launcher_patched && s.enabled;

export type Conflict = { other: string; files: number; wins: boolean; examples: string[] };
export type Save = { mode: string; name: string; mods: string[]; modified: number };
export type ServersAndSaves = { servers: string[]; server_dir: string; saves: Save[] };

export const api = {
  load: () => invoke<View>("load"),
  save: (rows: Row[]) => invoke<View>("save", { rows }),
  install: (sources: string[], enable = true) => invoke<View>("install", { sources, enable }),
  updateAll: () => invoke<View>("update_all"),
  remove: (ids: string[]) => invoke<View>("remove", { ids }),
  rollback: (id: string) => invoke<View>("rollback", { id }),
  profileNew: (name: string) => invoke<View>("profile_new", { name }),
  profileUse: (name: string) => invoke<View>("profile_use", { name }),
  profileDelete: (name: string) => invoke<View>("profile_delete", { name }),
  setBuild: (build: 41 | 42 | null) => invoke<View>("set_build", { build }),
  details: (id: string) => invoke<Details | null>("details", { id }),
  poster: (id: string) => invoke<ArrayBuffer>("poster", { id }),
  browse: (search: string, sort: string, page: number, build: number | null) =>
    invoke<Browse>("workshop_browse", { search, sort, page, build }),
  zbStatus: () => invoke<ZbStatus>("zb_status"),
  zbPreview: (remove: boolean) => invoke<string[]>("zb_preview", { remove }),
  zbInstall: () => invoke<View>("zb_install"),
  zbUninstall: () => invoke<View>("zb_uninstall"),
  findRequirement: (modId: string, neededBy: string) =>
    invoke<Candidate[]>("find_requirement", { modId, neededBy }),
  installMissing: () => invoke<[View, string[]]>("install_missing"),
  checkUpdates: () => invoke<string[]>("check_updates"),
  updateMods: (ids: string[]) => invoke<View>("update_mods", { ids }),
  conflicts: (id: string) => invoke<Conflict[]>("conflicts", { id }),
  serversAndSaves: () => invoke<ServersAndSaves>("servers_and_saves"),
  serverExport: (name: string) => invoke<string[]>("server_export", { name }),
  serverImport: (name: string) => invoke<View>("server_import", { name }),
  saveUse: (mode: string, name: string) => invoke<View>("save_use", { mode, name }),
  openPath: (path: string) => invoke<void>("open_path", { path }),
  openUrl: (url: string) => invoke<void>("open_url", { url }),
};

/** Human label + page URL for a source string from the backend. */
export function describeSource(source: string): { label: string; url: string | null } {
  const [kind, rest = ""] = source.split(/:(.*)/s);
  const workshop = (id: string) => `https://steamcommunity.com/sharedfiles/filedetails/?id=${id}`;
  switch (kind) {
    case "workshop":
      return { label: "Workshop", url: workshop(rest) };
    case "steam":
      return { label: "Subscribed", url: workshop(rest) };
    case "github":
      return { label: "GitHub", url: `https://github.com/${rest.split("@")[0]}` };
    case "local":
      return { label: "Local file", url: null };
    default:
      return { label: "Manual", url: null };
  }
}
