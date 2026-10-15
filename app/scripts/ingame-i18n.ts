// The in-game mod manager's translations. Run: node scripts/ingame-i18n.ts
// PZMM - Client's Translate/EN/UI.json (Build 42's format: a flat JSON of "UI_PZMM_<key>": "text") is the source:
// its strings go into en.json's "ingame" block, so they're translated with the app's other strings. Then each
// locale's "ingame" block (English too, so the two never drift) becomes its Translate/<LANG>/UI.json, same keys and %1 %2 placeholders (English
// where a string isn't translated yet; those are listed). It writes only en.json and those Translate folders.
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";

const locales = new URL("../src/lib/locales/", import.meta.url);
const translate = new URL("../../companion/PZMMClient/Contents/mods/PZMMClient/common/media/lua/shared/Translate/", import.meta.url);
// the app's locale -> the game's folder (CN is Simplified Chinese, CH Traditional)
const GAME: Record<string, string> = { en: "EN", de: "DE", es: "ES", fr: "FR", it: "IT", ko: "KO", pl: "PL", "pt-BR": "PTBR", ru: "RU", "zh-CN": "CN" };

const read = (url: URL) => JSON.parse(readFileSync(url, "utf8"));
const write = (url: URL, data: unknown, indent: number) => writeFileSync(url, JSON.stringify(data, null, indent) + "\n");

const source: Record<string, string> = read(new URL("EN/UI.json", translate));
const english = Object.fromEntries(Object.entries(source).map(([k, v]) => [k.replace(/^UI_PZMM_/, ""), v]));
const en = read(new URL("en.json", locales));
en.ingame = english;
write(new URL("en.json", locales), en, 2);

const holes = (s: string) => (s.match(/%\d+/g) ?? []).sort().join();
for (const [lang, folder] of Object.entries(GAME)) {
  const theirs: Record<string, string> = read(new URL(`${lang}.json`, locales)).ingame ?? {};
  const ui: Record<string, string> = {};
  const missing: string[] = [];
  for (const [key, text] of Object.entries(english)) {
    const t = theirs[key];
    // a translation with other placeholders than English's would pass the wrong values: English instead
    const ok = typeof t === "string" && holes(t) === holes(text) && t.split("\n").length === text.split("\n").length;
    if (!ok) missing.push(key);
    ui[`UI_PZMM_${key}`] = ok ? t : text;
  }
  mkdirSync(new URL(`${folder}/`, translate), { recursive: true });
  write(new URL(`${folder}/UI.json`, translate), ui, 4);
  if (missing.length) console.log(`${lang}: ${missing.length} in English: ${missing.join(", ")}`);
}
console.log(`ingame-i18n: ${Object.keys(english).length} strings, ${Object.keys(GAME).length} languages`);
