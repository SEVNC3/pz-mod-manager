import { invoke } from "@tauri-apps/api/core";

export type ModRow = { kind: "mod"; id: string; name: string; source: string; on: boolean; missing: boolean };
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
