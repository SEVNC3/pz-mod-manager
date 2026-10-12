// Kinds of server log line for the console's chips (LogView `chips`). Self-check: node scripts/logkind-check.ts
import type { LogLine } from "./api";

/** The server console's line kinds the chips pick. */
export const KINDS = ["error", "warn", "chat", "admin", "mods"] as const;
export type Kind = (typeof KINDS)[number];
const ADMIN = /\b(admin|rcon|kick(ed|user)?|(un)?ban(ned|user|id|ip)?|voiceban|teleport(ed|to|player)?|access ?level|whitelist|godmod\w*|invisible\w*|noclip|add(item|xp|vehicle|user)|servermsg|changeoption)\b/i;
/** The game's debug types for scripts and mods (the word after `LOG  :`), or a line about mods. */
const MODS = /^\s*\w+\s*:\s*(Mod|Lua|LuaObject|Script|Checksum)\b|\bmods?\b|\bworkshop\b|\.lua\b/i;

/** Whether a log line is of `kind`: by its level, or by what it says. */
export function isKind(l: LogLine, kind: Kind) {
  switch (kind) {
    case "error": return l.level === "error" || !!l.note;
    case "warn": return l.level === "warn";
    case "chat": return /ChatMessage|\bchat\b/i.test(l.text);
    case "admin": return ADMIN.test(l.text);
    case "mods": return MODS.test(l.text);
  }
}
