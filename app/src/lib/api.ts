import { invoke } from "@tauri-apps/api/core";
import { workshopUrl } from "./format";
import { t } from "./i18n.svelte";

export type Mode = "zomboid" | "viewpoint";
/** The game a profile kind runs: per-server profiles ("server") are plain Project Zomboid. */
export const modeOf = (kind: string | undefined): Mode => (kind === "viewpoint" ? "viewpoint" : "zomboid");
/** Project Viewpoint (core pz/viewpoint.rs): its mod id, Workshop item and community Discord. */
export const VIEWPOINT = { mod: "Viewpoint", workshop: "3809306528", discord: "https://discord.gg/g83XvPhEgQ" };
export type ModRow = {
  kind: "mod";
  id: string;
  name: string;
  source: string;
  on: boolean;
  missing: boolean;
  /** The profile didn't list it yet (installed since): appended, off. */
  new?: boolean;
  requires: string[];
  incompatible: string[];
  load_after: string[];
  load_before: string[];
  /** Mod Load Order Sorter's loadFirst / loadLast (mod.info or its rules file): "on" (whole list), "category" or "". */
  load_first: string;
  load_last: string;
  /** The Sorter's rules file pins its place (PZMM's Pin writes there). */
  pinned: boolean;
  author: string;
  only41: boolean;
  only42: boolean;
  /** Languages it has texts in (its `Translate/<LANG>` folders: EN, RU, PTBR...). */
  languages: string[];
};
type SepRow = { kind: "sep"; name: string };
export type Row = ModRow | SepRow;

export type View = {
  profile: string;
  profiles: string[];
  /** Kind of the active profile. */
  mode: Mode;
  /** Profile name -> kind ("server" profiles are Project Zomboid ones: see modeOf). */
  kinds: Record<string, string>;
  /** Profile name -> the server (its address) it was set up for. */
  servers: Record<string, string>;
  /** The library: what's installed, organised by the player (its order doesn't change what loads). */
  rows: Row[];
  /** The load order: enabled mod ids, as written to the game. */
  order: string[];
  adopted: boolean;
  /** First run: this load created the first profile. */
  created: boolean;
  zomboid: string;
  workshop: string | null;
  /** The build the profile runs (its forced one, else the game's), and the one the profile forces (null: follows the game). */
  build: 41 | 42 | null;
  forced_build: number | null;
  game_version: string | null;
  steam_branch: string | null;
  /** The game's language, as its Translate folders name it ("" until the game saved its options). */
  language: string;
};

export type ModDetails = {
  id: string;
  name: string;
  source: string;
  dir: string;
  info: Record<string, string>;
  ours: boolean;
  has_backup: boolean;
  /** It adds maps on the build being played. */
  maps: boolean;
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
/** One Workshop item in "What's new" (see pz::feed). */
export type FeedEntry = {
  item: string;
  title: string;
  mods: string[];
  updated: number;
  notes: ChangeNote[];
  loaded: boolean;
  /** The mod's own changelog (newest entry first, BBCode or else plain/Markdown text) when Steam has no notes. */
  bundled: { entries: { title: string; text: string }[]; links: [string, string][]; bbcode: boolean } | null;
  risks: ("map" | "version" | "notes")[];
  from: string;
  to: string;
  pending: boolean;
};
/** workshop.txt's fields (see pz::authoring). */
export type Upload = { title: string; description: string; tags: string[]; visibility: string; id: string };
export type AuthoringReport = { upload: boolean; findings: Finding[]; draft: Upload; tags: string[]; workshop: string; has_poster: boolean };
export type Built = { folder: string; skipped: string[]; findings: Finding[] };
/** Project Zomboid's official Steam posts (see pz::news); `error`: why this is an older copy. */
export type News = { fetched: number; posts: { title: string; date: number; excerpt: string; build: string; url: string; image: string }[]; error: string };
export type Feed = { since: number; basis: "launch" | "save" | ""; entries: FeedEntry[]; unloaded: number; error: string };
export type Comments = { total: number; comments: { author: string; time: number; text: string }[] };
export type Browse = { total: number; items: WorkshopItem[] };
/** Autosort's sections in list order, and each mod's section. */
export type Categories = { order: string[]; of: Record<string, string>; kinds: Record<string, string[]> };
/** A mod list the game's mod menu or a Workshop mod manager saved: `from` "modManager", "game" or "server". */
export type Preset = { from: string; name: string; mods: string[]; workshop: string[] };
/** Their presets, and the mods starred or hidden there. */
/** `tags`: the player's own tags, by mod id (PZMM's; every profile shares them). */
export type Marks = { presets: Preset[]; favourites: string[]; hidden: string[]; tags: Record<string, string[]> };
/** One mod's rule in Mod Load Order Sorter's rules file. `load_first` / `load_last`: "on" (whole list), "category" or "". */
export type Rule = { load_after: string[]; load_before: string[]; incompatible: string[]; load_first: string; load_last: string; category: string };
export type Candidate = Omit<WorkshopItem, "have"> & { confirmed: boolean; required_by_author: boolean; fits_build: boolean };

export type ZbStatus = {
  game_dir: string | null;
  have_mod: boolean;
  files_current: boolean;
  launcher_patched: boolean;
  enabled: boolean;
  required_by: string[];
  /** Other Java mod loaders installed for the game. `started_by`: "steam" (its launch options), the launcher config or script that starts it, or null. */
  loaders: { name: "leaf" | "avrix"; version: string; started_by: string | null }[];
};
/** An enabled Java mod (core pz/javamods.rs): `risks` are warnings to look at and notes worth knowing. */
export type JavaMod = {
  id: string;
  name: string;
  jar: string;
  sha256: string | null;
  signature: "unsigned" | "valid" | "invalid" | "unknown_key" | "missing";
  signer: string | null;
  signer_name: string | null;
  approved: boolean | null;
  risks: Finding[];
  /** What loads it; signatures and approvals are ZombieBuddy's only. */
  loader: "zombiebuddy" | "leaf" | "avrix";
};
export const zbReady = (s: ZbStatus | null) => !!s && s.have_mod && s.files_current && s.launcher_patched && s.enabled;

export type Shared = { name: string; kind: string; list: string; order: string[]; workshop: Record<string, string>; maps: string[] };

/** A share code: "PZMM1:" + base64 of the JSON, so it pastes cleanly into chat. */
export function encodeShare(s: Shared): string {
  const bytes = new TextEncoder().encode(JSON.stringify(s));
  let bin = "";
  for (const b of bytes) bin += String.fromCharCode(b);
  return "PZMM1:" + btoa(bin);
}

/** Accepts a share code or the raw JSON. */
export function decodeShare(text: string): Shared {
  const code = text.trim();
  let s;
  try {
    // cut short or mistyped, it isn't base64 or JSON
    s = JSON.parse(code.startsWith("PZMM1:") ? new TextDecoder().decode(Uint8Array.from(atob(code.slice(6)), (c) => c.charCodeAt(0))) : code);
  } catch {}
  if (typeof s?.name !== "string" || typeof s?.list !== "string") throw new Error(t("api.badShareCode"));
  return { name: s.name, kind: s.kind ?? "", list: s.list, order: Array.isArray(s.order) ? s.order : [], workshop: s.workshop ?? {}, maps: Array.isArray(s.maps) ? s.maps : [] };
}

type ModErrors = { name: string; count: number; example: string; id: string | null };
/** A game or server log (core pz/logs.rs): its first real error (`ids`: the installed mods it points at) and the errors per mod. */
export type LogReport = { file: string; first: { line: number; text: string; ids: string[] } | null; mods: ModErrors[] };
/** One error as it repeats in a log (1-based `line` first and `last` time); `ids`: the installed mods it points at. */
type ErrorGroup = { line: number; last: number; count: number; text: string; ids: string[] };
/** A log for the viewer: its last lines (`seq`: 0-based line in the file), errors grouped, errors per mod. */
export type LogView = { file: string; size: number; lines: LogLine[]; groups: ErrorGroup[]; mods: ModErrors[] };
/** `key`: doctor.<key> in the locale files, worded with `vars` (`text` is the English); `action`: a fix to offer (install a missing mod, disable an incompatible one, dismiss a scan's findings about a file by its SHA-256). */
export type Finding = { level: "error" | "warning" | "note"; subject: string; text: string; key: string; vars: Record<string, string>; action: { install?: string; disable?: string; dismiss?: string } | null; hidden: boolean };
export type Snapshot = { id: string; time: number; label: string; enabled: number; mods: number };
/** What writing a server config would change; `maps_to` equals `maps_from` when Map= stays. */
type ServerPreview = { new: boolean; added: string[]; removed: string[]; maps_from: string[]; maps_to: string[]; items_added: string[]; items_removed: string[] };
/** `created`: when the mod's folder was made (installed or subscribed), 0 if unknown. */
export type Stats = { size: number; modified: number; created: number };
export type Overlap = { wins_over: string[]; loses_to: string[] };
export type Conflict = { other: string; files: number; wins: boolean; examples: string[] };
export type Save = { mode: string; name: string; mods: string[]; modified: number };
export type ServersAndSaves = { servers: string[]; server_dir: string; saves: Save[]; deleted: [string, string][]; own_folder: string | null };
/** Zero-byte map files: cells the game rebuilds once removed, and world files only a backup brings back. */
export type SaveDetails = { backups: ServerBackup[]; broken: { cells: string[]; other: string[] }; resets: Record<string, number> };
/** The game's memory, PZ_Optimization and what the mod list weighs (core pz/perf.rs). */
export type PerfView = {
  ram_mb: number;
  /** ProjectZomboid64.json; null when the game wasn't found. */
  launcher: string | null;
  memory_mb: number | null;
  memory_by_pzopt: boolean;
  suggested_mb: number;
  mods: number;
  mods_size: number;
  /** The largest enabled mods: id, name, bytes. */
  largest: [string, string, number][];
  pzopt: { game_revision: string | null; installed: string | null; installer: string | null; installer_revision: string | null };
  game_running: boolean;
  /** The garbage collector ("Z", "G1", "Shenandoah"...) and the other JVM options; null when the game wasn't found. */
  jvm: { gc: string; args: string[] } | null;
};
/** The active profile's launch options (core profile.rs Launch). */
export type ProfileLaunch = { profile: string; launch: { args: string; cache_dir: string | null } };
export type SavePlayer = { id: number; name: string; username: string; x: number; y: number; z: number; dead: boolean; data_size: number };

// --- hosting (core pz/host.rs) ---
type ServerStatus = "stopped" | "starting" | "running" | "stopping" | "crashed";
/** A known log line in plain words: `key` is hosting.note.<key>, `text` the English. */
type LogNote = { key: string; text: string; vars: Record<string, string> };
export type LogLine = { seq: number; text: string; level: "info" | "warn" | "error"; note: LogNote | null };
export type ServerCard = {
  name: string;
  /** PZMM manages it; else a config in the Zomboid folder, read-only until adopted. */
  managed: boolean;
  build: string;
  install_dir: string;
  cachedir: string;
  ini: string;
  game_port: number;
  direct_port: number;
  memory_mb: number;
  admin_user: string;
  status: ServerStatus;
  /** PZMM started it this session: commands go to its console. */
  attached: boolean;
  started: number;
  memory_used: number;
  players: number | null;
  reason: LogNote | null;
  auto: ServerAuto;
  /** Run by LinuxGSM or Docker: PZMM manages its files, never starts or stops it. */
  external: External | null;
  /** Workshop items updated since it started, and when the restart countdown ends (0: none). */
  updates: string[];
  restart_at: number;
  /** Its starts put the PZMM list in its description. */
  tag: boolean;
  /** Mods marked "server" (only it needs them) or "optional"; the rest are needed on both sides. */
  sides: Record<string, ModSide>;
  /** Run elsewhere, reached over RCON at this `host:port`: console and players only. */
  remote: string | null;
  /** The next planned restart while it runs (0: none). */
  next_restart: number;
  /** CPU (percent) and memory (bytes) over the last minutes, oldest first. */
  samples: [number, number][];
  /** The in-game date at its last save: year, month, day, hour, minute. */
  game_time: [number, number, number, number, number] | null;
  /** Run elsewhere: the commands that update and check its files there, [what, command]. */
  upkeep: [string, string][];
  /** The address players join at, when set (a tunnel's). */
  address: string;
  /** Run elsewhere: its files over FTP, when set. */
  ftp: Ftp | null;
  /** Another server uses its data folder too, so its Discord chat relay stays off. */
  shares_logs: boolean;
};
export type ModSide = "server" | "optional";
/** A server run elsewhere (core pz/host.rs Remote). */
export type RemoteServer = { name: string; host: string; port: number; build: string };
/** File access to a rented server (core pz/ftp.rs); the password is in the credential store. */
export type Ftp = { host: string; port: number; user: string; plain: boolean; folder: string; server: string; pulled: number; fingerprint: string };
export type FtpEntry = { name: string; dir: boolean; size: number };
/** The PZMM list for a server's description: the form that fits ("" and "none" when nothing does). */
export type TagView = { tag: string; form: "full" | "items" | "none"; length: number; limit: number };
/** A server's automation (core pz/autopilot.rs). */
export type ServerAuto = {
  updates: "always" | "empty" | "scheduled" | "notify";
  check_minutes: number;
  warn_minutes: number[];
  restarts: string[];
  restart_hours: number;
  save_minutes: number;
  broadcasts: { minutes: number; text: string }[];
  backup_hours: number;
  backup_on_start: boolean;
  backup_keep: number;
  backup_max_mb: number;
  crash_restarts: number;
  discord: { saved: boolean; events: string[]; message_id: string; bot: boolean; channel: string; role: string; relay: boolean };
};
type External = { kind: "linuxgsm" | "docker"; folder: string; run: string[] };
export type FoundServer = { name: string; data: string; install: string | null; external: External };
export type ServerBackup = { file: string; time: number; size: number; reason: string; label: string };
/** A request from the game (PZMM - Client writes them; the core checks them). */
export type GameRequest = { action: "install"; ids: string[] } | { action: "backup"; mode: string; name: string } | { action: "check" } | { action: "open"; tab: string };
/** What the game last reported: the mods it loaded, at the menu or in a game. */
export type GameReport = { client: string; game: string; state: string; at: number; mods: string[]; save: string | null; server: string | null };
export type BackupView = { backups: ServerBackup[]; empty_cells: string[]; game_on_start: boolean; game_count: string; game_dir: string };
/** A form field from a server file's own comments (core pz/server_config.rs). */
export type Setting = {
  key: string;
  value: string;
  section: string;
  description: string;
  kind: "bool" | "number" | "choice" | "text" | "secret";
  choices: [string, string][];
  min: number | null;
  max: number | null;
  default: string | null;
  /** A mod's name for its option; "" for the game's own. */
  label: string;
  /** The mod it comes from; "" for the game's own. */
  source: string;
};
export type SpawnRegion = { name: string; file: string; server_file: boolean };
export type ConfigView = {
  ini: Setting[];
  sandbox: Setting[];
  sandbox_error: string;
  regions: SpawnRegion[];
  regions_error: string;
  /** Spawn regions the server's map mods offer that it doesn't list yet. */
  spawn_offers: SpawnRegion[];
  /** Files (by suffix) with an earlier version to go back to. */
  prev: string[];
  presets: string[];
  own_presets: string[];
};
/** What happened to a server in the last 24 hours (core pz/autopilot.rs). */
export type ServerEvent24 = { server: string; time: number; kind: "mods" | "restart" | "aborted" | "crash" | "server"; text: string };
export type ChunkArea = { x1: number; y1: number; x2: number; y2: number };
export type ItemState = { id: string; title: string; updated: number; downloaded: number | null };
export type HostView = { servers: ServerCard[]; ram_mb: number; memory_mb: number; install_b42: string; install_41: string; zomboid: string; server_dir: string | null };
/** A server's mod lines. */
export type ServerLines = { mods: string[]; workshop: string[]; maps: string[] };
/** `key`: hosting.check.<key>; errors block the write. */
export type LineProblem = { error: boolean; key: string; id: string; item: string; text: string };
/** A server's lines checked: Workshop titles, each mod's [name, item], and required items the list lacks ([needed by, item]). */
type LineReport = { problems: LineProblem[]; titles: Record<string, string>; mods: Record<string, [string, string]>; required: [string, string][] };
/** A server's accounts and bans (core pz/saves.rs accounts). */
type Account = { username: string; role: string; last: string; steamid: string; banned: boolean };
export type Accounts = { users: Account[]; banned_ids: [string, string][]; banned_ips: [string, string, string][] };
export type People = { accounts: Accounts | null; accounts_error: string; characters: SavePlayer[] };
/** What the owner keeps about a server's players (core pz/dossier.rs): notes and tags by username, joins and leaves oldest first. */
export type Dossier = { players: Record<string, { note: string; tags: string[] }>; history: { time: number; name: string; joined: boolean; event?: "ban" | "unban" | "kick" }[] };
/** The world's cells for a server's player map: `size` tiles a cell. */
/** `maps`: the server's map mods installed here, bottom first (see MapGrid `worlds`). */
export type WorldGrid = { size: number; cells: GridCell[]; maps: string[] };
/** The game's items and vehicles, `Module.Type` and English name (core pz/catalog.rs). */
type CatalogEntry = { id: string; name: string };
export type Catalog = { items: CatalogEntry[]; vehicles: CatalogEntry[] };
/** Workshop collections read without installing (core pz/collection.rs Pack). */
export type Pack = {
  title: string;
  items: string[];
  mods: string[];
  item_mods: Record<string, string[]>;
  titles: Record<string, string>;
  installed: string[];
  delisted: string[];
  maps: string[];
  presets: string[];
  notes: string[];
};
type NewServer = {
  name: string;
  build: string;
  install_dir: string | null;
  cachedir: string | null;
  game_port: number;
  direct_port: number;
  admin_user: string;
  admin_password: string;
  join_password: string;
  max_players: number;
  public: boolean;
  upnp: boolean;
  memory_mb: number;
  lines: ServerLines;
  address: string;
};
/** A "server" event: a log line or a status change. */
export type ServerEvent = { name: string; status: ServerStatus; line: LogLine | null; reason: LogNote | null };

/** settings.json (see core settings.rs); the GitHub token is never in it. */
export type Settings = {
  folders: Record<string, string>;
  update_check: "off" | "start" | "1h" | "3h";
  update_action: "notify" | "install";
  app_updates: boolean;
  workshop_links: "steam" | "browser";
  recycle: boolean;
  remove_requirements: "ask" | "always" | "never";
  backup_cap_mb: number;
  launch_args: string;
  autosort_new: boolean;
  keep_marks: boolean;
  save_backup_launch: boolean;
  save_backup_exit: boolean;
  save_backup_keep: number;
  tray: boolean;
  /** null: not chosen yet (asks, unless `tray` already says the tray). */
  close_ask: boolean | null;
  save_backup_every: number;
  countries: boolean;
  /** Start PZMM in the tray when signing in. */
  autostart: boolean;
  /** Desktop notifications on (while the window isn't in front). */
  notify: NotifyKind[];
};
export type NotifyKind = "crash" | "restart" | "downloads" | "favourites";
export type Folder = {
  key: string;
  label: string;
  env: string | null;
  path: string | null;
  /** The environment variable is set and wins over the setting. */
  overridden: boolean;
  /** Set by hand (else autodetected). */
  custom: boolean;
  detected: string | null;
  status: "found" | "missing" | "not_pz";
  warnings: string[];
};
/** A cell's [x, y]: B41 cells are 300 tiles, B42 cells 256, so only one build's cells compare. */
export type Cell = [number, number];
type MapFolder = { folder: string; title: string; lots: string[]; cells: Cell[] };
export type MapMod = { id: string; name: string; source: string; requires: string[]; maps: MapFolder[] };
/** A Workshop item PZMM read maps from (the Workshop scan or a check). */
/** `mods`: how many mods the item holds on this build (a mod pack holds several); times are Unix seconds. */
type MapItem = { title: string; author: string; subscriptions: number; size: number; manual: boolean; updated: number; created: number; tags: string[]; mods: number };
export type MapsView = {
  build: 41 | 42;
  vanilla_folders: string[];
  vanilla: Cell[] | null;
  /** Vanilla cells that are mostly river. */
  water: Cell[];
  mods: MapMod[];
  /** Workshop maps read without installing (one per mod id). */
  checked: MapMod[];
  items: Record<string, MapItem>;
  /** Mod ids several Workshop items provide, with those items. */
  providers: Record<string, string[]>;
};
/** What the in-game map draws, in world tiles: polygons per layer (rings of x, y pairs: the outline, then holes) and road lines. */
export type WorldMap = { water: number[][][]; roads: number[][][]; buildings: number[][][]; road_lines: number[][] };
/** A picture under the map grid: image pixel p is world tile p * scale + offset on each axis. */
export type MapBackground = { path: string; scale: [number, number]; offset: [number, number] };
export type GridCell = { x: number; y: number; kind: "vanilla" | "water" | "map" | "over" | "conflict"; maps: string[] };
export type MapScan = { phase: "listing" | "reading" | "done"; done: number; total: number; failed: number };
export type MapCompare = { only_server: { folder: string; id: string | null; item: string | null }[]; only_you: string[]; overlap: [string, string][] };

export type SettingsView = { settings: Settings; folders: Folder[]; token_saved: boolean; token_env: boolean; steam_key_saved: boolean; backups_size: number };

// --- the launcher (core pz/launcher.rs) ---
/** A server from Steam's list, filled in by A2S. `addr` answers queries; `port` is the game port. */
export type GameServer = {
  addr: string;
  port: number;
  name: string;
  /** Map=, cut to 31 characters by the server. */
  map: string;
  players: number;
  max_players: number;
  keywords: string;
  /** "42.21", "41.78.16" or "". */
  build: string;
  pvp: boolean;
  password: boolean;
  /** ms; null until it answered. */
  ping: number | null;
  /** Only the first ~127 characters of its Mods= line. */
  mods: string[];
  mod_count: number;
  description: string;
  /** The PZMM list in its description (core pz/tag.rs), and why one it carries can't be used. */
  tag: ServerTag | null;
  tag_error: string;
  rules: Record<string, string>;
  /** Steam friends playing on it now (the Friends list only). */
  friends: string[];
  probed: boolean;
  error: string;
};
/** The merged server list, and what each source gave: "steam" (the Steam client), "web" (the Web API key), "game" (the game's saved list). */
export type ServerListing = { servers: GameServer[]; sources: { source: "steam" | "web" | "game"; count: number; error: string }[] };
/** Steam's own list answered (through the client or the key), not only the game's saved one. */
export const fromSteam = (sources: ServerListing["sources"]) => sources.some((a) => a.source !== "game" && !a.error);
/** Workshop items with their update times when the list was made; mods and maps empty in the short form. */
type ServerTag = { items: [string, number][]; mods: string[]; maps: string[] };
type ResolvedMod = { id: string; status: "installed" | "found" | "likely" | "unknown"; item: string; title: string; size: number; updated: number; preview: string };
/** One of a PZMM list's items: `updated` is the Workshop's time now, `pinned` the one the list was made with. */
type ListItem = { item: string; title: string; size: number; updated: number; pinned: number; installed: boolean; preview: string };
/** `list`: the server's PZMM list, "full" (exact mods, order and maps), "items" (exact items only) or "". */
export type ServerMods = { mods: ResolvedMod[]; maps: string[]; build: 41 | 42; list: "" | "full" | "items"; items: ListItem[]; server_only: string[]; optional: string[] };
/** One join: the exact mod set used (`items`: Workshop item -> its update time then). */
export type ServerJoin = { time: number; mods: string[]; maps: string[]; items: Record<string, number> };
export type LauncherView = { favourites: GameServer[]; history: GameServer[]; joins: Record<string, ServerJoin[]>; key_saved: boolean; countries: boolean };
export type ServerPlayer = { name: string; duration: number };

/** `ip:port` to join. */
export const connectOf = (s: GameServer) => `${s.addr.replace(/:\d+$/, "")}:${s.port}`;

/** Where to join a server hosted here, and the join password its config sets: used as is, never shown. */
export async function ownJoin(c: ServerCard) {
  const password = (await api.hostText(c.name, ".ini").catch(() => "")).match(/^Password=(.*)$/m)?.[1]?.trim() ?? "";
  return { url: `127.0.0.1:${c.game_port}`, password };
}

/** Windows: Steam's list comes through the Steam client there; elsewhere only the Web API key gets it. */
export const WINDOWS = typeof navigator !== "undefined" && navigator.userAgent.includes("Windows");

/** `f`, asked again (backing off) while PZMM answers that it's busy with an install: waited out quietly. */
export async function patient<T>(f: () => Promise<T>): Promise<T> {
  for (let wait = 1000; ; wait = Math.min(wait * 2, 15_000)) {
    try {
      return await f();
    } catch (e) {
      if (!String(e).includes("errors.busy")) throw e; // the backend's keyed busy error (keyed! in src-tauri)
      await new Promise((r) => setTimeout(r, wait));
    }
  }
}

/** Mods a server's list says players may go without: left out until ticked. */
export const defaultSkip = (mods: ServerMods): Record<string, boolean> => Object.fromEntries([...mods.server_only, ...mods.optional].map((id) => [id, true]));

/** A Workshop item as the server setup popup shows it; `likely`: only a probable match for the server's mod (a guess),
 * `mods`: the server's mod IDs it stands for (left out of the profile when a guess is unticked). */
export type SetupItem = Pick<WorkshopItem, "id" | "title" | "preview_url" | "file_size"> & { likely?: boolean; mods?: string[] };
/** What a server's worked-out mods already say about their Workshop items: the setup popup opens with them. */
export const knownItems = (mods: ServerMods): SetupItem[] => [
  ...mods.items.map((i) => ({ id: i.item, title: i.title, preview_url: i.preview, file_size: i.size })),
  ...mods.mods.filter((m) => m.item).map((m) => ({ id: m.item, title: m.title, preview_url: m.preview, file_size: m.size,
    likely: mods.mods.every((x) => x.item !== m.item || x.status === "likely"), mods: mods.mods.filter((x) => x.item === m.item).map((x) => x.id) })),
];

/** What "Set up and join" takes: the Workshop items to download (a PZMM list's that aren't installed, else the
 * mods found, or with `likely` the likely ones too, that aren't) and the profile's mod order, without `skip`ped mods. */
export function joinPlan(server: GameServer, mods: ServerMods | null, skip: Record<string, boolean> = mods ? defaultSkip(mods) : {}, likely = false) {
  const itemSkipped = (item: string) => {
    const of = (mods?.mods ?? []).filter((m) => m.item === item);
    return of.length > 0 && of.every((m) => skip[m.id]);
  };
  const downloads: { item: string; size: number }[] = mods?.list
    ? mods.items.filter((i) => !i.installed && !itemSkipped(i.item))
    : (mods?.mods ?? []).filter((m) => !skip[m.id] && m.item && (m.status === "found" || (likely && m.status === "likely"))).filter((m, i, l) => l.findIndex((x) => x.item === m.item) === i);
  const order = (mods?.list === "full" ? mods.mods.map((m) => m.id) : server.mods).filter((id) => !skip[id]);
  return { downloads, order };
}

/** A server typed or pasted: `host:port`, `host` (port 16261) or a `pzmm://join/host:port[?password]` link. */
export function parseServer(text: string): { server: GameServer; password: string | null } | null {
  let s = text.trim();
  let password: string | null = null;
  const link = /^pzmm:\/\/join\/([^?#\s]+)(?:\?([^#\s]*))?\/?$/i.exec(s);
  if (link) {
    s = decodeURIComponent(link[1]).replace(/\/$/, "");
    const q = new URLSearchParams(link[2] ?? "");
    if (q.has("password")) password = q.get("password") ?? "";
  } else if (/^\w+:\/\//.test(s)) return null;
  const m = /^([A-Za-z0-9.-]+|\[[0-9a-fA-F:]+\])(?::(\d{1,5}))?$/.exec(s);
  const port = Number(m?.[2] ?? 16261);
  if (!m || port < 1 || port > 65535) return null;
  const server: GameServer = {
    addr: `${m[1]}:${port}`, port, name: `${m[1]}:${port}`, map: "", players: 0, max_players: 0, keywords: "", build: "", pvp: false, password: password !== null,
    ping: null, mods: [], mod_count: 0, description: "", tag: null, tag_error: "", rules: {}, friends: [], probed: false, error: "",
  };
  return { server, password };
}

export const api = {
  load: () => invoke<View>("load"),
  // the backend only stores order, on/off and separators; `profile`: the one the rows are from (refused if another is active)
  save: (rows: Row[], order: string[], profile: string) =>
    invoke<View>("save", { rows: rows.map((r) => (r.kind === "mod" ? { kind: r.kind, id: r.id, on: r.on } : { kind: r.kind, name: r.name })), order, profile }),
  install: (sources: string[]) => invoke<View>("install", { sources }),
  remove: (ids: string[]) => invoke<View>("remove", { ids }),
  /** The other profiles that enable any of `ids`, with how many. */
  profilesUsing: (ids: string[]) => invoke<Record<string, number>>("profiles_using", { ids }),
  rollback: (id: string) => invoke<View>("rollback", { id }),
  profileNew: (name: string) => invoke<View>("profile_new", { name }),
  profileUse: (name: string) => invoke<View>("profile_use", { name }),
  profileDelete: (name: string) => invoke<View>("profile_delete", { name }),
  profilesUnused: () => invoke<string[]>("profiles_unused"),
  profilesPrune: (names: string[]) => invoke<View>("profiles_prune", { names }),
  profileRename: (from: string, to: string) => invoke<View>("profile_rename", { from, to }),
  profileDuplicate: (from: string, to: string) => invoke<View>("profile_duplicate", { from, to }),
  /** Keep the active profile as saved, before a change (`label` says which). */
  snapshotCreate: (label: string) => invoke<void>("snapshot_create", { label }),
  snapshotList: () => invoke<Snapshot[]>("snapshot_list"),
  snapshotRestore: (id: string) => invoke<View>("snapshot_restore", { id }),
  setBuild: (build: 41 | 42 | null) => invoke<View>("set_build", { build }),
  details: (id: string) => invoke<ModDetails | null>("details", { id }),
  poster: (id: string) => invoke<ArrayBuffer>("poster", { id }),
  browse: (query: Query) => invoke<Browse>("workshop_browse", { query }),
  workshopInfo: (id: string) => invoke<WorkshopInfo>("workshop_info", { id }),
  workshopChanges: (id: string, page: number) => invoke<ChangeNote[]>("workshop_changes", { id, page }),
  /** Change notes since the installed version of mod `id`. */
  modChanges: (id: string) => invoke<ChangeNote[]>("mod_changes", { id }),
  feed: (notes: boolean) => invoke<Feed>("feed", { notes }),
  news: () => invoke<News>("news"),
  authoringCheck: (dir: string) => invoke<AuthoringReport>("authoring_check", { dir }),
  authoringPoster: (dir: string) => invoke<ArrayBuffer>("authoring_poster", { dir }),
  authoringBuild: (dir: string, parent: string, upload: Upload, preview: number[] | null) => invoke<Built>("authoring_build", { dir, parent, upload, preview }),
  workshopComments: (creator: string, id: string, start: number) => invoke<Comments>("workshop_comments", { creator, id, start }),
  workshopAuthor: (creator: string, page: number) => invoke<Browse>("workshop_author", { creator, page }),
  workshopItems: (ids: string[]) => invoke<WorkshopItem[]>("workshop_items", { ids }),
  zbStatus: () => invoke<ZbStatus>("zb_status"),
  zbPreview: (remove: boolean, viewpoint = false) => invoke<string[]>("zb_preview", { remove, viewpoint }),
  /** `viewpoint`: from the Viewpoint card, which enables ZombieBuddy only in a Viewpoint profile. */
  zbInstall: (viewpoint = false) => invoke<View>("zb_install", { viewpoint }),
  zbUninstall: () => invoke<View>("zb_uninstall"),
  /** The window's close button, answered: hide to the tray, or quit (asking first when servers run). */
  /** Hide a Check setup finding (back when its mod changes), or show it again. */
  findingHide: (f: Finding, on: boolean) => invoke<void>("finding_hide", { key: f.key, subject: f.subject, vars: f.vars, on }),
  zbUnblock: (sha: string) => invoke<void>("zb_unblock", { sha }),
  historyList: () => invoke<HistoryRecord[]>("history_list"),
  historyClear: (ids?: string[]) => invoke<HistoryRecord[]>("history_clear", { ids: ids ?? null }),
  /** The page is ready: close the start-up splash window and show this one. */
  splashDone: () => invoke<void>("splash_done"),
  appClose: (toTray: boolean) => invoke<void>("app_close", { toTray }),
  workshopSubscribe: (id: string, on: boolean, modId?: string, name?: string) => invoke<View>("workshop_subscribe", { id, on, modId: modId ?? null, name: name ?? null }),
  javaMods: () => invoke<JavaMod[]>("java_mods"),
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
  perfView: () => invoke<PerfView>("perf_view"),
  gameRunning: () => invoke<boolean>("game_running"),
  perfMemorySet: (mb: number) => invoke<void>("perf_memory_set", { mb }),
  perfJvmSet: (gc: string, args: string[]) => invoke<void>("perf_jvm_set", { gc, args }),
  profileLaunch: () => invoke<ProfileLaunch>("profile_launch"),
  profileLaunchSet: (args: string, ownFolder: boolean) => invoke<ProfileLaunch>("profile_launch_set", { args, ownFolder }),
  /** Run PZ_Optimization's installer (or its uninstall); resolves to what it printed. */
  pzoptRun: (uninstall: boolean) => invoke<string>("pzopt_run", { uninstall }),
  saveDetails: (mode: string, name: string) => invoke<SaveDetails>("save_details", { mode, name }),
  /** A `slot` name keeps the backup until it's deleted. */
  saveBackup: (mode: string, name: string, slot: string) => invoke<void>("save_backup", { mode, name, slot }),
  /** What the game asked PZMM to do since the last look (PZMM - Client), taken: each comes once. */
  gameRequests: () => invoke<GameRequest[]>("game_requests"),
  /** What the game last reported (PZMM - Client's game.json). */
  gameReport: () => invoke<GameReport | null>("game_report"),
  saveRestore: (mode: string, name: string, file: string) => invoke<void>("save_restore", { mode, name, file }),
  saveBackupDelete: (mode: string, name: string, file: string) => invoke<void>("save_backup_delete", { mode, name, file }),
  saveRepair: (mode: string, name: string) => invoke<number>("save_repair", { mode, name }),
  /** Give the save the active profile's mods. */
  saveSetMods: (mode: string, name: string) => invoke<void>("save_set_mods", { mode, name }),
  /** A save's map_sand.bin as the sandbox form (only `sandbox`, `presets` and `own_presets` are filled). */
  saveSandbox: (mode: string, name: string) => invoke<ConfigView>("save_sandbox", { mode, name }),
  saveSandboxWrite: (mode: string, name: string, changes: [string, string][]) => invoke<void>("save_sandbox_write", { mode, name, changes }),
  saveSandboxPreset: (preset: string, own: boolean) => invoke<Record<string, string>>("save_sandbox_preset", { preset, own }),
  savePlayers: (mode: string, name: string) => invoke<SavePlayer[]>("save_players", { mode, name }),
  /** Give a backup a slot name; `copy` adds a named copy instead. */
  saveBackupName: (mode: string, name: string, file: string, slot: string, copy: boolean) => invoke<void>("save_backup_name", { mode, name, file, slot, copy }),
  /** The save's screenshot, or a backup's (`file`); empty when there's none. */
  saveThumb: (mode: string, name: string, file = "") => invoke<ArrayBuffer>("save_thumb", { mode, name, file }),
  saveReset: (mode: string, name: string, what: string) => invoke<number>("save_reset", { mode, name, what }),
  /** Copy the save with the active profile's mods: the copy's name. */
  saveClone: (mode: string, name: string) => invoke<string>("save_clone", { mode, name }),
  /** The save's databases: [file, [table, rows][]][]. */
  saveDatabases: (mode: string, name: string) => invoke<[string, [string, number][]][]>("save_databases", { mode, name }),
  /** 100 rows of a table from `offset`; data shows as its size. */
  saveTable: (mode: string, name: string, file: string, table: string, offset: number) =>
    invoke<{ columns: string[]; rows: (string | number | null | { bytes: number })[][] }>("save_table", { mode, name, file, table, offset }),
  serverMaps: (name: string) => invoke<string[]>("server_maps", { name }),
  serverPreview: (name: string, maps: string[]) => invoke<ServerPreview>("server_preview", { name, maps }),
  serverCheck: (name: string, maps: string[]) => invoke<LineProblem[]>("server_check", { name, maps }),
  serverExport: (name: string, maps: string[]) => invoke<string[]>("server_export", { name, maps }),
  serverImport: (name: string) => invoke<View>("server_import", { name }),
  hostView: () => invoke<HostView>("host_view"),
  /** Which of the (port, UDP?) pairs are free. */
  hostPorts: (ports: [number, boolean][]) => invoke<boolean[]>("host_ports", { ports }),
  /** Install (when needed), write the config, first start; "progress" events count the install in percent. */
  hostCreate: (spec: NewServer) => invoke<void>("host_create", { new: spec }),
  hostPreflight: (name: string) => invoke<Finding[]>("host_preflight", { name }),
  hostStart: (name: string) => invoke<void>("host_start", { name }),
  hostStop: (name: string) => invoke<void>("host_stop", { name }),
  hostRestart: (name: string) => invoke<void>("host_restart", { name }),
  hostLog: (name: string, after: number) => invoke<{ attached: boolean; lines: LogLine[] }>("host_log", { name, after }),
  /** null: it went to the console (the reply shows in the log); else RCON's reply. */
  hostSend: (name: string, command: string) => invoke<string | null>("host_send", { name, command }),
  hostPlayers: (name: string) => invoke<string[]>("host_players", { name }),
  hostLines: (name: string) => invoke<ServerLines>("host_lines", { name }),
  hostLinesFromProfile: (profile: string, build: number) => invoke<[ServerLines, string[]]>("host_lines_from_profile", { profile, build }),
  hostLinesFromCollection: (input: string, build: number) => invoke<[ServerLines, string[]]>("host_lines_from_collection", { input, build }),
  hostCheck: (lines: ServerLines) => invoke<LineReport>("host_check", { lines }),
  hostLinesWith: (lines: ServerLines, input: string, build: number, first: boolean) => invoke<[ServerLines, string[]]>("host_lines_with", { lines, input, build, first }),
  hostPeople: (name: string) => invoke<People>("host_people", { name }),
  hostCatalog: (name: string) => invoke<Catalog>("host_catalog", { name }),
  hostDossier: (name: string) => invoke<Dossier>("host_dossier", { name }),
  hostPlayerSet: (name: string, player: string, note: string, tags: string[]) => invoke<Dossier>("host_player_set", { name, player, note, tags }),
  hostWorld: (name: string) => invoke<WorldGrid>("host_world", { name }),
  hostRemoteAdd: (remote: RemoteServer, password: string) => invoke<void>("host_remote_add", { remote, password }),
  hostRemoteForget: (name: string) => invoke<void>("host_remote_forget", { name }),
  hostPreview: (name: string, lines: ServerLines) => invoke<ServerPreview>("host_preview", { name, lines }),
  hostWrite: (name: string, lines: ServerLines) => invoke<void>("host_write", { name, lines }),
  /** The PZMM list for `lines` fitted to the server's description; `write` puts it in the config. */
  hostTag: (name: string, lines: ServerLines, write: boolean) => invoke<TagView>("host_tag", { name, lines, write }),
  hostTagSet: (name: string, on: boolean) => invoke<void>("host_tag_set", { name, on }),
  hostSidesSet: (name: string, sides: Record<string, ModSide>) => invoke<void>("host_sides_set", { name, sides }),
  /** The build is read from the install. */
  hostAdopt: (name: string, installDir: string) => invoke<void>("host_adopt", { name, installDir }),
  hostForget: (name: string) => invoke<void>("host_forget", { name }),
  hostAutoSet: (name: string, auto: ServerAuto) => invoke<void>("host_auto_set", { name, auto }),
  hostCheckUpdates: (name: string) => invoke<string[]>("host_check_updates", { name }),
  hostBackups: (name: string) => invoke<BackupView>("host_backups", { name }),
  hostBackup: (name: string) => invoke<void>("host_backup", { name }),
  hostRestore: (name: string, file: string) => invoke<void>("host_restore", { name, file }),
  hostZones: (name: string) => invoke<ServerBackup[]>("host_zones", { name }),
  hostZonesSave: (name: string) => invoke<ServerBackup>("host_zones_save", { name }),
  hostZonesRestore: (name: string, file: string) => invoke<void>("host_zones_restore", { name, file }),
  hostConfig: (name: string) => invoke<ConfigView>("host_config", { name }),
  hostIniWrite: (name: string, changes: [string, string][]) => invoke<void>("host_ini_write", { name, changes }),
  hostSandboxWrite: (name: string, changes: [string, string][]) => invoke<void>("host_sandbox_write", { name, changes }),
  hostSpawnWrite: (name: string, regions: SpawnRegion[]) => invoke<void>("host_spawn_write", { name, regions }),
  hostPreset: (name: string, preset: string, own: boolean) => invoke<Record<string, string>>("host_preset", { name, preset, own }),
  hostPresetSave: (preset: string, values: Record<string, string>) => invoke<void>("host_preset_save", { preset, values }),
  hostDiscordSet: (name: string, url: string) => invoke<void>("host_discord_set", { name, url }),
  hostDiscordTest: (name: string) => invoke<void>("host_discord_test", { name }),
  hostDiscordBotSet: (name: string, token: string) => invoke<void>("host_discord_bot_set", { name, token }),
  hostDiscordBotTest: (name: string) => invoke<void>("host_discord_bot_test", { name }),
  hostAddressSet: (name: string, address: string) => invoke<void>("host_address_set", { name, address }),
  /** Log in (an empty password keeps the saved one), keep the settings, list the folder. */
  hostFtpSet: (name: string, ftp: Ftp, password: string) => invoke<[string, FtpEntry[]]>("host_ftp_set", { name, ftp, password }),
  hostFtpBrowse: (name: string, dir: string) => invoke<[string, FtpEntry[]]>("host_ftp_browse", { name, dir }),
  hostFtpForget: (name: string) => invoke<void>("host_ftp_forget", { name }),
  hostFtpPull: (name: string) => invoke<number>("host_ftp_pull", { name }),
  /** The backup of the host's copies, when it had any. */
  hostFtpPush: (name: string) => invoke<string | null>("host_ftp_push", { name }),
  hostFtpWorld: (name: string) => invoke<ServerBackup>("host_ftp_world", { name }),
  hostFtpStop: () => invoke<void>("host_ftp_stop"),
  hostDetect: (folder: string) => invoke<FoundServer[]>("host_detect", { folder }),
  hostAddExternal: (found: FoundServer) => invoke<void>("host_add_external", { found }),
  /** One of a server's settings files as text, by suffix (".ini", "_SandboxVars.lua"...). */
  hostText: (name: string, file: string) => invoke<string>("host_text", { name, file }),
  hostTextWrite: (name: string, file: string, text: string) => invoke<void>("host_text_write", { name, file, text }),
  /** Put a settings file back as it was before its latest change. */
  hostTextRestore: (name: string, file: string) => invoke<void>("host_text_restore", { name, file }),
  hostUpdate: (name: string, build: string) => invoke<void>("host_update", { name, build }),
  hostAbortRestart: (name: string) => invoke<boolean>("host_abort_restart", { name }),
  hostHistory: (name: string) => invoke<ServerEvent24[]>("host_history", { name }),
  hostDiagnostics: (name: string) => invoke<string>("host_diagnostics", { name }),
  /** The newest chat or perk log: its file and its lines in short. */
  hostGameLog: (name: string, kind: "chat" | "perk") => invoke<[string | null, string[]]>("host_game_log", { name, kind }),
  hostClone: (from: string, to: string) => invoke<void>("host_clone", { from, to }),
  hostCoopWorlds: () => invoke<string[]>("host_coop_worlds"),
  hostMigrate: (name: string, world: string) => invoke<void>("host_migrate", { name, world }),
  hostBundle: (name: string, dir: string) => invoke<string>("host_bundle", { name, dir }),
  hostWipe: (name: string, keepCharacters: boolean, keepZones: boolean) => invoke<number>("host_wipe", { name, keepCharacters, keepZones }),
  /** [chunks a reset would take, chunks in the world]. */
  hostChunkPlan: (name: string, areas: ChunkArea[], inside: boolean) => invoke<[number, number]>("host_chunk_plan", { name, areas, inside }),
  hostChunkReset: (name: string, areas: ChunkArea[], inside: boolean) => invoke<number>("host_chunk_reset", { name, areas, inside }),
  hostBackupExport: (name: string, file: string, dir: string) => invoke<string>("host_backup_export", { name, file, dir }),
  hostBackupImport: (name: string, file: string) => invoke<void>("host_backup_import", { name, file }),
  hostWorkshopStatus: (name: string) => invoke<ItemState[]>("host_workshop_status", { name }),
  trayWords: (open: string, quit: string, hidden: string) => invoke<void>("tray_words", { open, quit, hidden }),
  notify: (text: string) => invoke<void>("notify", { text }),
  /** Quit after the "servers are running" question: stop them first, or leave them running. */
  appQuit: (stop: boolean) => invoke<void>("app_quit", { stop }),
  saveUse: (mode: string, name: string) => invoke<View>("save_use", { mode, name }),
  mapView: () => invoke<MapsView>("map_view"),
  mapWorld: (id: string) => invoke<WorldMap>("map_world", { id }),
  mapGrid: (vanilla: Cell[], water: Cell[], shown: MapMod[]) => invoke<GridCell[]>("map_grid", { vanilla, water, shown }),
  /** `fixed`: the enabled maps, which stay (listed or not). */
  mapQuickSelect: (all: MapMod[], fixed: MapMod[], ranks: Record<string, number>) => invoke<string[]>("map_quick_select", { all, fixed, ranks }),
  /** The Map= list for `folders`: `order`'s arrangement, the profile's saved one for maps it lacks. */
  mapLine: (order: string[], folders: string[]) => invoke<string[]>("map_line", { order, folders }),
  /** Refused (maps.err.*) unless `profile`, the one it was arranged for, is still active. */
  mapOrderSave: (maps: string[], profile: string) => invoke<void>("map_order_save", { maps, profile }),
  /** `labels`: the export's words in the UI language (`ids` fills {id} and {folders}, `missing` {ids}). */
  mapExport: (shown: MapMod[], mapLine: string[], labels: { ids: string; required: string; missing: string }) => invoke<string>("map_export", { shown, mapLine, labels }),
  mapCheck: (id: string, download: boolean) => invoke<MapMod[]>("map_check", { id, download }),
  mapCheckStop: () => invoke<void>("map_check_stop"),
  /** Read every Map-tagged Workshop item, both builds ("maps-scan" events report progress). */
  mapScan: () => invoke<MapScan>("map_scan"),
  mapScanStop: () => invoke<void>("map_scan_stop"),
  mapPresets: () => invoke<Record<string, string[]>>("map_presets"),
  mapPresetSet: (name: string, ids: string[] | null) => invoke<Record<string, string[]>>("map_preset_set", { name, ids }),
  mapBackground: (build: number) => invoke<MapBackground | null>("map_background", { build }),
  /** null forgets it. */
  mapBackgroundSet: (build: number, bg: MapBackground | null) => invoke<void>("map_background_set", { build, bg }),
  mapBackgroundImage: (path: string) => invoke<ArrayBuffer>("map_background_image", { path }),
  /** A PNG or JPEG (null: cancelled). */
  pickImage: () => invoke<string | null>("pick_image"),
  /** `theirs`: a public server's mods and maps, compared instead of config `name`. */
  mapServerCompare: (name: string, yours: string[], theirs?: { mods: string[]; workshop: string[]; maps: string[] }, build?: number) =>
    invoke<MapCompare>("map_server_compare", { name, yours, theirs: theirs ?? null, build: build ?? null }),
  /** `online`: also ask the Workshop about the enabled Workshop mods (gone, hidden, not the original). */
  doctor: (online = false) => invoke<Finding[]>("doctor", { online }),
  /** Stop reporting the scans' findings about the file with this SHA-256 (until it changes). */
  scanDismiss: (sha: string) => invoke<void>("scan_dismiss", { sha }),
  logReports: () => invoke<LogReport[]>("log_reports"),
  /** `file` (else the first log) unless its size is still `known`. */
  gameLog: (file: string | null, known: number) => invoke<{ files: string[]; view: LogView | null }>("game_log", { file, known }),
  profileExport: () => invoke<Shared>("profile_export"),
  profileImport: (shared: Shared) => invoke<View>("profile_import", { shared }),
  collectionPreview: (input: string) => invoke<Pack>("collection_preview", { input }),
  collectionProfile: (title: string, items: string[], skip: string[]) => invoke<View>("collection_profile", { title, items, skip }),
  lists: () => invoke<Marks>("lists"),
  presetSave: (preset: Preset) => invoke<Marks>("preset_save", { preset }),
  presetImport: (preset: Preset) => invoke<View>("preset_import", { preset }),
  modMark: (id: string, kind: "favourite" | "hidden", on: boolean) => invoke<Marks>("mod_mark", { id, kind, on }),
  modPin: (id: string, after: string[], before: string[]) => invoke<View>("mod_pin", { id, after, before }),
  modTags: (id: string, tags: string[]) => invoke<Marks>("mod_tags", { id, tags }),
  modRule: (id: string) => invoke<Rule>("mod_rule", { id }),
  modRuleSet: (id: string, rule: Rule) => invoke<View>("mod_rule_set", { id, rule }),
  modePreview: (mode: Mode) => invoke<string[]>("mode_preview", { mode }),
  modeSwitch: (mode: Mode) => invoke<View>("mode_switch", { mode }),
  settingsGet: () => invoke<SettingsView>("settings_get"),
  settingsSet: (settings: Settings) => invoke<void>("settings_set", { settings }),
  /** An empty path goes back to autodetect; `moveData` moves PZMM's data along. */
  folderSet: (key: string, path: string, moveData: boolean) => invoke<SettingsView>("folder_set", { key, path, moveData }),
  pickFolder: (start: string) => invoke<string | null>("pick_folder", { start }),
  /** .zip files (or, with `folder`, a folder) to install. */
  pickInstall: (folder: boolean) => invoke<string[]>("pick_install", { folder }),
  /** Cancel the Workshop download in progress ("progress" events count it). */
  installStop: () => invoke<void>("install_stop"),
  hostInstallStop: () => invoke<void>("host_install_stop"),
  maskUser: (text: string) => invoke<string>("mask_user", { text }),
  githubTokenSet: (token: string) => invoke<void>("github_token_set", { token }),
  backupsClear: () => invoke<number>("backups_clear"),
  diagnostics: () => invoke<string>("diagnostics"),
  launchGame: () => invoke<void>("launch_game"),
  /** Every PZ server: Steam's list (the Web API key, else the Steam client) and the game's saved one, merged; kept two minutes unless `fresh`. */
  serversList: (fresh: boolean) => invoke<ServerListing>("servers_list", { fresh }),
  serversFriends: () => invoke<GameServer[]>("servers_friends"),
  /** A2S info and rules for each (32 at a time, cached five minutes unless `fresh`). */
  serversProbe: (servers: GameServer[], fresh: boolean) => invoke<GameServer[]>("servers_probe", { servers, fresh }),
  serverPlayers: (addr: string) => invoke<ServerPlayer[]>("server_players", { addr }),
  /** Country codes by IP (only when Settings countries is on). */
  serverCountries: (ips: string[]) => invoke<Record<string, string>>("server_countries", { ips }),
  serverCountriesSet: (on: boolean) => invoke<void>("server_countries_set", { on }),
  serverMods: (server: GameServer) => invoke<ServerMods>("server_mods", { server }),
  /** Read the Workshop items updated since the mod ID index was made (in the background, at most daily). */
  modIndexRefresh: () => invoke<void>("mod_index_refresh"),
  /** Download `items`, then make and use the "Server <name>" profile from `mods` and `maps`. */
  serverSetup: (server: GameServer, items: string[], mods: string[], maps: string[]) => invoke<View>("server_setup", { server, items, mods, maps }),
  /** Start the game with +connect (and +password: "" uses a remembered one). */
  serverJoin: (server: GameServer, password: string, remember: boolean) => invoke<void>("server_join", { server, password, remember }),
  launcherView: () => invoke<LauncherView>("launcher_view"),
  serverFavourite: (server: GameServer, on: boolean) => invoke<Omit<LauncherView, "key_saved">>("server_favourite", { server, on }),
  serverPasswordSaved: (connect: string) => invoke<boolean>("server_password_saved", { connect }),
  serverPasswordForget: (connect: string) => invoke<void>("server_password_forget", { connect }),
  steamKeySet: (key: string) => invoke<void>("steam_key_set", { key }),
  /** pzmm:// links PZMM was started with (later ones come as "join-link" events). */
  joinLinks: () => invoke<string[]>("join_links"),
  // failures are logged by the backend (they reach the activity log), so callers needn't handle them
  openPath: (path: string) => invoke<void>("open_path", { path }).catch(() => {}),
  openUrl: (url: string) => invoke<void>("open_url", { url }).catch(() => {}),
};

/** Installed by PZMM (so it can update and remove it), not a Steam subscription or a manual install. */
/** A download history record: `source` is `workshop:<id>`, `steam:<id>` (a subscription), `github:...` or `local:...`. */
export type HistoryRecord = { id: string; name: string; source: string; action: "installed" | "updated" | "removed" | "subscribed" | "unsubscribed"; at: number };

export const isOurs = (m: ModRow) => /^(workshop|github|local):/.test(m.source);
/** Remove can take it: PZMM's own, or a folder added to Zomboid/mods by hand (that goes to the Recycle Bin). */
export const removable = (m: ModRow) => !m.missing && (isOurs(m) || m.source === "manual");

/** Human label + page URL for a source string from the backend. */
export function describeSource(source: string): { label: string; url: string | null } {
  const [kind, rest = ""] = source.split(/:(.*)/s);
  switch (kind) {
    case "workshop":
      return { label: t("source.workshop"), url: workshopUrl(rest) };
    case "steam":
      return { label: t("source.steam"), url: workshopUrl(rest) };
    case "github":
      return { label: "GitHub", url: `https://github.com/${rest.split("@")[0]}` };
    case "local":
      return { label: t("source.local"), url: null };
    default:
      return { label: t("source.manual"), url: null };
  }
}
