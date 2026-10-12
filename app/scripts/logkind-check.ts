// Self-check for src/lib/logkind.ts on server console lines as the game writes them. Run: node scripts/logkind-check.ts
import assert from "node:assert/strict";
import { isKind, KINDS } from "../src/lib/logkind.ts";
import type { LogLine } from "../src/lib/api.ts";

const line = (text: string, level: LogLine["level"] = "info", note: LogLine["note"] = null): LogLine => ({ seq: 0, text, level, note });
const kinds = (l: LogLine) => KINDS.filter((k) => isKind(l, k)).join(" ");

const cases: [LogLine, string][] = [
  [line("ERROR: General      f:0, t:1760000000000, st:1,234> ExceptionLogger.logException> Exception thrown java.lang.NullPointerException", "error"), "error"],
  [line("WARN : Mod          f:0, t:1760000000000, st:1,234> ChooseGameInfo.readModInfo> mod.info has no id", "warn"), "warn mods"],
  [line("LOG  : Lua          f:0, t:1760000000000, st:1,234> Loading: media/lua/server/MyMod.lua"), "mods"],
  [line("LOG  : General      f:0, t:1760000000000, st:1,234> [chat] Got message:ChatMessage{chat=General, author='Kate', text='hi'}"), "chat"],
  [line("LOG  : General      f:0, t:1760000000000, st:1,234> admin kicked user Kate"), "admin"],
  [line("LOG  : Network      f:0, t:1760000000000, st:1,234> RCON: command banuser \"Kate\""), "admin"],
  [line("LOG  : General      f:0, t:1760000000000, st:1,234> Workshop: item 2392709985 is up to date"), "mods"],
  [line("LOG  : General      f:0, t:1760000000000, st:1,234> *** SERVER STARTED ****", "info", { key: "started", text: "", vars: {} }), "error"],
  [line("LOG  : General      f:0, t:1760000000000, st:1,234> Saving world..."), ""],
  [line("LOG  : General      f:0, t:1760000000000, st:1,234> Loading model mode"), ""],
];
for (const [l, want] of cases) assert.equal(kinds(l), want, l.text);
console.log(`logkind-check: ${cases.length} lines ok`);
